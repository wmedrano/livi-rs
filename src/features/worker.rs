use core::ffi::c_void;
use std::mem::size_of;
use std::slice;
use std::sync::{Arc, Mutex};

use ringbuf::traits::{Consumer, Observer, Producer, Split};

pub(crate) type WorkerMessageSender = ringbuf::HeapProd<u8>;
pub(crate) type WorkerMessageReceiver = ringbuf::HeapCons<u8>;

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
    let (sender, receiver) = ringbuf::HeapRb::new(MAX_MESSAGE_SIZE * N_MESSAGES).split();
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
    let body = if size == 0 {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(body, size) }
    };
    let total_size = size_of::<usize>() + size;
    let size_as_bytes = size.to_be_bytes();
    let send_result = if sender.vacant_len() < total_size {
        return lv2_sys::LV2_Worker_Status_LV2_WORKER_ERR_NO_SPACE;
    } else {
        sender.push_iter(size_as_bytes.iter().chain(body.iter()).copied())
    };
    // PANIC OK: We ensure capacity above so this should never panic.
    assert_eq!(send_result, total_size);
    lv2_sys::LV2_Worker_Status_LV2_WORKER_SUCCESS
}

fn pop_message(receiver: &mut WorkerMessageReceiver) -> WorkerMessage {
    let mut size_as_bytes = [0; size_of::<usize>()];
    assert_eq!(receiver.pop_slice(&mut size_as_bytes), size_as_bytes.len());
    let size = usize::from_be_bytes(size_as_bytes);
    let mut body: MessageBody = [0; MAX_MESSAGE_SIZE];
    assert_eq!(receiver.pop_slice(&mut body[..size]), size);

    WorkerMessage { size, body }
}

/// Queue a worker message from an LV2 audio-context callback.
///
/// # Safety
/// `handle` must be the live schedule handle supplied by livi to this plugin
/// instance. Calls using that handle must be serialized and confined to the
/// calling contexts allowed by the LV2 Worker extension. For a nonzero `size`
/// within the supported message limit, `body` must point to at least `size`
/// initialized, readable bytes for the duration of this call. A zero-size
/// message may use a null body pointer. Neither pointer is retained as message
/// data: the payload is copied into the queue.
pub unsafe extern "C" fn schedule_work(
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

unsafe impl Send for Worker {}
unsafe impl Sync for Worker {}

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
        while *plugin_is_alive && self.receiver.occupied_len() >= size_of::<usize>() {
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

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("plugin_is_alive", &self.plugin_is_alive)
            .field("interface", &self.interface)
            .field("instance_handle", &self.instance_handle)
            .field("receiver", &"__internal__")
            .field("sender", &"__internal__")
            .finish()
    }
}

// Not real-time safe.
pub(crate) unsafe fn maybe_get_worker_interface(
    plugin: &lilv::plugin::Plugin,
    common_uris: &crate::CommonUris,
    instance: &mut lilv::instance::ActiveInstance,
) -> Option<lv2_sys::LV2_Worker_Interface> {
    unsafe {
        if !plugin.has_feature(&common_uris.worker_schedule_feature_uri) {
            return None;
        }
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
}

// Run this in the real-time thread
// to process responses from the async worker.
pub(crate) fn handle_work_responses(
    worker_interface: &mut lv2_sys::LV2_Worker_Interface,
    receiver: &mut WorkerMessageReceiver,
    handle: lv2_sys::LV2_Handle,
) {
    while receiver.occupied_len() >= size_of::<usize>() {
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

/// Use a WorkerManager to own and run Workers. The WorkerManager will drop
/// workers automatically once their associated plugin Instance has been
/// dropped.
///
/// #### Example usage:
/// ```
/// # use livi;
///
/// # let world = livi::World::new();
/// # const MIN_BLOCK_SIZE: usize = 1;
/// # const MAX_BLOCK_SIZE: usize = 256;
/// # const SAMPLE_RATE: f64 = 44100.0;
/// # let plugin = world
/// #     .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
/// #     .expect("Plugin not found.");
/// let features = world.try_build_features(livi::FeaturesBuilder{
///     min_block_length: MIN_BLOCK_SIZE,
///     max_block_length: MAX_BLOCK_SIZE,
/// }).expect("invalid feature block bounds");
/// let mut instance = unsafe {
///     plugin
///         .instantiate(features.clone(), SAMPLE_RATE)
///         .expect("Could not instantiate plugin.")
/// };
/// ```
#[derive(Default, Debug)]
pub struct WorkerManager {
    new_workers: Mutex<Vec<Worker>>,
    // Workers that may be in the process of running are kept in a different
    // variable to prevent blocking when adding new workers.
    running_workers: Mutex<Vec<Worker>>,
}

impl WorkerManager {
    /// Run all the workers that have been added and are alive. This function
    /// should not be run in the Realtime thread. Additionally, there is no
    /// benefit to running it in parallel as concurrency is limited to 1 worker
    /// at a time.
    pub fn run_workers(&self) {
        let mut workers = self.running_workers.lock().unwrap();
        workers.extend(self.new_workers.lock().unwrap().drain(..));
        workers.iter_mut().for_each(|worker| worker.do_work());
        workers.retain(|worker| worker.should_keep_working());
    }

    /// The number of workers that are currently alive.
    pub fn workers_count(&self) -> usize {
        self.running_workers.lock().unwrap().len() + self.new_workers.lock().unwrap().len()
    }

    pub(crate) fn add_worker(&self, worker: Worker) {
        self.new_workers.lock().unwrap().push(worker);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    #[test]
    fn empty_worker_messages() {
        unsafe extern "C" fn count_work(
            handle: lv2_sys::LV2_Handle,
            _respond: lv2_sys::LV2_Worker_Respond_Function,
            _respond_handle: lv2_sys::LV2_Worker_Respond_Handle,
            size: u32,
            _data: *const c_void,
        ) -> lv2_sys::LV2_Worker_Status {
            assert_eq!(size, 0);
            unsafe {
                *(handle as *mut usize) += 1;
            }
            lv2_sys::LV2_Worker_Status_LV2_WORKER_SUCCESS
        }

        unsafe extern "C" fn count_response(
            handle: lv2_sys::LV2_Handle,
            size: u32,
            _data: *const c_void,
        ) -> lv2_sys::LV2_Worker_Status {
            assert_eq!(size, 0);
            unsafe {
                *(handle as *mut usize) += 1;
            }
            lv2_sys::LV2_Worker_Status_LV2_WORKER_SUCCESS
        }

        let (mut sender, receiver) = instantiate_queue();
        let (response_sender, mut response_receiver) = instantiate_queue();
        let mut count = 0usize;
        let handle = (&mut count as *mut usize).cast();
        let mut interface = lv2_sys::LV2_Worker_Interface {
            work: Some(count_work),
            work_response: Some(count_response),
            end_run: None,
        };
        let mut worker = Worker::new(
            Arc::new(Mutex::new(true)),
            interface,
            handle,
            receiver,
            response_sender,
        );
        for data in [
            std::ptr::NonNull::<u8>::dangling().as_ptr(),
            std::ptr::null_mut(),
        ] {
            assert_eq!(
                publish_message(&mut sender, 0, data),
                lv2_sys::LV2_Worker_Status_LV2_WORKER_SUCCESS
            );
            worker.do_work();
            assert_eq!(worker.receiver.occupied_len(), 0);
            assert_eq!(
                publish_message(&mut worker.sender, 0, data),
                lv2_sys::LV2_Worker_Status_LV2_WORKER_SUCCESS
            );
            handle_work_responses(&mut interface, &mut response_receiver, handle);
            assert_eq!(response_receiver.occupied_len(), 0);
        }
        assert_eq!(count, 4);
        let mut byte = 42;
        assert_eq!(
            publish_message(&mut sender, 1, &mut byte),
            lv2_sys::LV2_Worker_Status_LV2_WORKER_SUCCESS
        );
        let message = pop_message(&mut worker.receiver);
        assert_eq!(&message.body[..message.size], &[42]);
    }

    #[test]
    fn worker_request_and_response_wraparound() {
        for callback in [
            schedule_work as unsafe extern "C" fn(_, _, _) -> _,
            worker_respond,
        ] {
            let (mut sender, mut receiver) = instantiate_queue();
            for round in 0..12u8 {
                let data = [round + 1; MAX_MESSAGE_SIZE];
                assert_eq!(
                    unsafe {
                        callback(
                            (&mut sender as *mut WorkerMessageSender).cast(),
                            data.len() as u32,
                            data.as_ptr().cast(),
                        )
                    },
                    lv2_sys::LV2_Worker_Status_LV2_WORKER_SUCCESS
                );
                let message = pop_message(&mut receiver);
                assert_eq!(message.size, data.len());
                assert!(message.body == data, "corrupted message {round}");
                assert_eq!(receiver.occupied_len(), 0);
            }
        }
    }

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
