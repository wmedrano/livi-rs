use core::ffi::c_void;
use ringbuf::{Consumer, Producer, RingBuffer};
use std::mem::size_of;
use std::slice;
use std::sync::{Arc, Mutex};

pub(crate) type WorkerMessageSender = Producer<u8>;
pub(crate) type WorkerMessageReceiver = Consumer<u8>;

const MAX_MESSAGE_SIZE: usize = 8192;
const N_MESSAGES: usize = 4;

type MessageBody = [u8; MAX_MESSAGE_SIZE];

#[derive(Debug)]
struct WorkerMessage {
    size: usize,
    body: MessageBody,
}

impl WorkerMessage {
    fn data(&mut self) -> *mut c_void {
        &mut self.body as *mut MessageBody as *mut c_void
    }
}

pub(crate) fn instantiate_queue() -> (WorkerMessageSender, WorkerMessageReceiver) {
    let (sender, receiver) = RingBuffer::<u8>::new(MAX_MESSAGE_SIZE * N_MESSAGES).split();
    (sender, receiver)
}

fn publish_message(
    sender: &mut WorkerMessageSender,
    size: usize,
    body: *mut u8,
) -> lv2_sys::LV2_Worker_Status {
    if size > MAX_MESSAGE_SIZE {
        return lv2_sys::LV2_Worker_Status_LV2_WORKER_ERR_NO_SPACE;
    }
    let mut body = unsafe { slice::from_raw_parts(body, size) };
    let total_size = size_of::<usize>() + size;
    if sender.remaining() < total_size {
        return lv2_sys::LV2_Worker_Status_LV2_WORKER_ERR_NO_SPACE;
    }
    let size_as_bytes = size.to_be_bytes();
    sender.push_slice(&size_as_bytes);
    let result = sender.read_from(&mut body, Some(size));
    match result {
        Ok(_) => lv2_sys::LV2_Worker_Status_LV2_WORKER_SUCCESS,
        Err(_) => lv2_sys::LV2_Worker_Status_LV2_WORKER_ERR_UNKNOWN,
    }
}

fn pop_message(receiver: &mut WorkerMessageReceiver) -> WorkerMessage {
    let mut size_as_bytes = [0; size_of::<usize>()];
    receiver.pop_slice(&mut size_as_bytes);
    let size = usize::from_be_bytes(size_as_bytes);
    let mut body: MessageBody = [0; MAX_MESSAGE_SIZE];
    let mut slice = &mut body[..];
    receiver.write_into(&mut slice, Some(size)).unwrap();
    WorkerMessage { size, body }
}

pub extern "C" fn schedule_work(
    handle: lv2_sys::LV2_Worker_Schedule_Handle,
    size: u32,
    body: *const c_void,
) -> lv2_sys::LV2_Worker_Status {
    let sender = unsafe { &mut *(handle as *mut WorkerMessageSender) };
    publish_message(sender, size as usize, body as *mut u8)
}

extern "C" fn worker_respond(
    handle: lv2_sys::LV2_Worker_Respond_Handle,
    size: u32,
    body: *const c_void,
) -> lv2_sys::LV2_Worker_Status {
    let sender = unsafe { &mut *(handle as *mut WorkerMessageSender) };
    publish_message(sender, size as usize, body as *mut u8)
}

/// A plugin instance delegates non-realtime-safe
/// work to a Worker, which performs the work
/// asynchronously in another thread before
/// sending the results back to the plugin.
///
/// The worker itself is easy to use. Once you obtain
/// a worker from the plugin, just call worker.do_work()
/// periodically and that's it. Currently there's no method
/// to "wait" on work and only perform work when messages arrive,
/// you have to keep calling do_work while the plugin is alive.
pub struct Worker {
    plugin_is_alive: Arc<Mutex<bool>>,
    interface: lv2_sys::LV2_Worker_Interface,
    instance_handle: lv2_sys::LV2_Handle,
    receiver: WorkerMessageReceiver, // Where we find work to do
    sender: WorkerMessageSender,     // Where we send the results of our work
}

impl Worker {
    pub(crate) fn new(
        plugin_is_alive: Arc<Mutex<bool>>,
        interface: lv2_sys::LV2_Worker_Interface,
        instance_handle: lv2_sys::LV2_Handle,
        receiver: WorkerMessageReceiver,
        sender: WorkerMessageSender,
    ) -> Self {
        Worker {
            plugin_is_alive,
            interface,
            instance_handle,
            receiver,
            sender,
        }
    }

    /// Run this in a non-realtime thread
    /// to do non-realtime work and send
    /// the results back to the realtime thread.
    pub fn do_work(&mut self) {
        let plugin_is_alive = self.plugin_is_alive.lock().unwrap();
        while *plugin_is_alive && self.receiver.len() > size_of::<usize>() {
            let mut message = pop_message(&mut self.receiver);
            if let Some(work_function) = self.interface.work {
                let sender = &mut self.sender as *mut WorkerMessageSender as *mut c_void;
                unsafe {
                    work_function(
                        self.instance_handle,
                        Some(worker_respond),
                        sender,
                        message.size as u32,
                        message.data(),
                    )
                };
            }
        }
    }

    /// Keep the worker working as long as this
    /// remains true. Once this returns false,
    /// you can drop the worker.
    pub fn should_keep_working(&self) -> bool {
        *self.plugin_is_alive.lock().unwrap()
    }
}

pub(crate) unsafe fn maybe_get_worker_interface(
    instance: &mut lilv::instance::ActiveInstance,
) -> Option<lv2_sys::LV2_Worker_Interface> {
    // TODO: Remove below after
    // https://github.com/poidl/lv2_raw/issues/4 is fixed.
    let descriptor = instance.instance().descriptor().unwrap();
    type ExtDataFn = extern "C" fn(uri: *const u8) -> *const c_void;
    let extension_data: Option<ExtDataFn> = std::mem::transmute(descriptor.extension_data);
    extension_data?;
    // Delete up to here.
    Some(
        *instance
            .instance()
            .extension_data::<lv2_sys::LV2_Worker_Interface>(
                "http://lv2plug.in/ns/ext/worker#interface",
            )?
            .as_ref(),
    )
}

// Run this in the real-time thread
// to process responses from the async worker.
pub(crate) fn handle_work_responses(
    worker_interface: &mut lv2_sys::LV2_Worker_Interface,
    receiver: &mut WorkerMessageReceiver,
    handle: lv2_sys::LV2_Handle,
) {
    while receiver.len() > size_of::<usize>() {
        let mut message = pop_message(receiver);
        if let Some(work_response_function) = worker_interface.work_response {
            unsafe { work_response_function(handle, message.size as u32, message.data()) };
        }
    }
}

// Run this in the real-time thread
// to indicate all work responses have
// been handled.
pub(crate) fn end_run(
    worker_interface: &mut lv2_sys::LV2_Worker_Interface,
    handle: lv2_sys::LV2_Handle,
) {
    if let Some(end_function) = worker_interface.end_run {
        unsafe { end_function(handle) };
    }
}

/// Use a WorkerManager to own and run
/// Workers. The WorkerManager will drop
/// workers automatically once their associated
/// plugin Instance has been dropped.
///
/// #### Example usage:
/// ```
/// # use livi;
///
/// # let mut world = livi::World::new();
/// # const MIN_BLOCK_SIZE: usize = 1;
/// # const MAX_BLOCK_SIZE: usize = 256;
/// # const SAMPLE_RATE: f64 = 44100.0;
/// # world
/// #     .initialize_block_length(MIN_BLOCK_SIZE, MAX_BLOCK_SIZE)
/// #     .unwrap();
/// # let plugin = world
/// #     .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
/// #     .expect("Plugin not found.");
/// let mut instance = unsafe {
///     plugin
///         .instantiate(SAMPLE_RATE)
///         .expect("Could not instantiate plugin.")
/// };
///
/// let mut worker_manager = livi::WorkerManager::default();
/// if let Some(worker) = instance.take_worker() {
///     worker_manager.add_worker(worker);
/// }
///
/// // Call this periodically to drive workers.
/// worker_manager.run_workers();
/// ```
#[derive(Default)]
pub struct WorkerManager {
    workers: std::vec::Vec<Worker>,
}

impl WorkerManager {
    pub fn add_worker(&mut self, worker: Worker) {
        self.workers.push(worker);
    }

    pub fn run_workers(&mut self) {
        self.workers.iter_mut().for_each(|worker| worker.do_work());
        self.workers.retain(|worker| worker.should_keep_working());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    #[test]
    fn test_send() {
        let (mut sender, mut receiver) = instantiate_queue();
        let sentence_to_transfer = String::from("This is a message for you");
        let mut data = sentence_to_transfer.clone().into_bytes();
        publish_message(&mut sender, data.len(), data.as_mut_ptr());
        let message = pop_message(&mut receiver);
        let body = &message.body[..message.size];
        let message_body = str::from_utf8(body).unwrap();
        assert_eq!(sentence_to_transfer, message_body);
    }
}
