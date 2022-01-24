use core::ffi::c_void;
use ringbuf::{Consumer, Producer, RingBuffer};
use std::mem::size_of;
use std::slice;

pub(crate) type WorkerMessageSender = Producer<u8>;
pub(crate) type WorkerMessageReceiver = Consumer<u8>;

const MAX_MESSAGE_SIZE: usize = 512;
type MessageBody = [u8; MAX_MESSAGE_SIZE];

#[derive(Debug)]
struct WorkerMessage {
    size: usize,
    body: MessageBody,
}

impl WorkerMessage {
    fn get_actual_size(&self) -> usize {
        size_of::<Self>() - MAX_MESSAGE_SIZE + self.size
    }

    fn data(&mut self) -> *mut c_void {
        &mut self.body as *mut MessageBody as *mut c_void
    }
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

pub(crate) fn instantiate_queue() -> (WorkerMessageSender, WorkerMessageReceiver) {
    let (sender, receiver) = RingBuffer::<u8>::new(8192).split();
    (sender, receiver)
}

fn do_work(
    worker_interface: &mut lv2_sys::LV2_Worker_Interface,
    receiver: &mut WorkerMessageReceiver,
    handle: lv2_sys::LV2_Handle,
    sender: &mut WorkerMessageSender,
) {
    while receiver.len() > size_of::<usize>() {
        let mut message = pop_message(receiver);
        if let Some(work_function) = worker_interface.work {
            let sender = sender as *mut WorkerMessageSender as *mut c_void;
            let body = message.data();
            unsafe {
                work_function(
                    handle,
                    Some(worker_respond),
                    sender,
                    message.size as u32,
                    body,
                )
            };
        }
    }
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
///
/// The worker must be dropped before dropping the plugin instance.
/// I need to learn more about rust lifetimes to see how we can
/// make the worker safer to use.
pub struct Worker {
    interface: lv2_sys::LV2_Worker_Interface,
    instance_handle: lv2_sys::LV2_Handle,
    receiver: WorkerMessageReceiver, // Where we find work to do
    sender: WorkerMessageSender,     // Where we send the results of our work
}

impl Worker {
    pub fn new(
        interface: lv2_sys::LV2_Worker_Interface,
        instance_handle: lv2_sys::LV2_Handle,
        receiver: WorkerMessageReceiver,
        sender: WorkerMessageSender,
    ) -> Self {
        Worker {
            interface,
            instance_handle,
            receiver,
            sender,
        }
    }

    /// Run this in a NON-real-time thread
    /// to do non-realtime work and send
    /// the results back to the realtime thread.
    pub fn do_work(&mut self) {
        do_work(
            &mut self.interface,
            &mut self.receiver,
            self.instance_handle,
            &mut self.sender,
        );
    }
}

pub(crate) unsafe fn maybe_get_worker_interface(
    instance: &mut lilv::instance::ActiveInstance,
) -> Option<lv2_sys::LV2_Worker_Interface> {
    Some(
        *(instance
            .instance()
            .extension_data::<lv2_sys::LV2_Worker_Interface>(
                std::str::from_utf8(lv2_sys::LV2_WORKER__interface).unwrap(),
            )?
            .as_ptr()),
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
            let body_ptr = message.data();
            unsafe { work_response_function(handle, message.size as u32, body_ptr) };
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    #[test]
    fn test_get_actual_size() {
        let message = WorkerMessage {
            size: 100,
            body: [0; MAX_MESSAGE_SIZE],
        };
        let expected_size = 100 + size_of::<usize>();
        assert_eq!(message.get_actual_size(), expected_size);
    }

    #[test]
    fn test_send() {
        let ringbuffer = RingBuffer::<u8>::new(8192);
        let (mut sender, mut receiver) = ringbuffer.split();
        let sentence_to_transfer = String::from("This is a message for you");
        let mut data = sentence_to_transfer.clone().into_bytes();
        publish_message(&mut sender, data.len(), data.as_mut_ptr());
        let message = pop_message(&mut receiver);
        let body = &message.body[..message.size];
        let message_body = str::from_utf8(body).unwrap();
        assert_eq!(sentence_to_transfer, message_body);
    }
}
