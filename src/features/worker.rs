use core::ffi::c_void;
use ringbuf::{Consumer, Producer, RingBuffer};
use std::io::Read;
use std::mem::size_of;
use std::slice;
use std::str;

type WorkerMessageSender = Producer<u8>;
type WorkerMessageReceiver = Consumer<u8>;

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
    let result = receiver.write_into(&mut slice, Some(size)).unwrap();
    WorkerMessage {
        size: size,
        body: body,
    }
}

unsafe extern "C" fn schedule_work(
    handle: lv2_sys::LV2_Worker_Schedule_Handle,
    size: u32,
    body: *const c_void,
) -> lv2_sys::LV2_Worker_Status {
    let sender = &mut *(handle as *mut WorkerMessageSender);
    publish_message(sender, size as usize, body as *mut u8)
}

unsafe extern "C" fn worker_respond(
    handle: lv2_sys::LV2_Worker_Respond_Handle,
    size: u32,
    body: *const c_void,
) -> lv2_sys::LV2_Worker_Status {
    let sender = &mut *(handle as *mut WorkerMessageSender);
    publish_message(sender, size as usize, body as *mut u8)
}

// Run this in a NON-real-time thread
// to do non-realtime work and send
// the results back to the realtime thread.
fn maybe_do_work(
    worker_interface: &mut lv2_sys::LV2_Worker_Interface,
    receiver: &mut WorkerMessageReceiver,
    handle: lv2_sys::LV2_Handle,
    sender: &mut WorkerMessageSender,
) {
    while receiver.len() > size_of::<usize>() {
        let mut message = pop_message(receiver);
        if let Some(work_function) = worker_interface.work {
            let sender_ptr = sender as *mut WorkerMessageSender as *mut c_void;
            let body_ptr = message.data();
            unsafe {
                work_function(
                    handle,
                    Some(worker_respond),
                    sender_ptr,
                    message.size as u32,
                    body_ptr,
                )
            };
        }
    }
}

// Run this in the real-time thread
// to process responses from the async worker.
fn handle_work_responses(
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
fn end_run(worker_interface: &mut lv2_sys::LV2_Worker_Interface, handle: lv2_sys::LV2_Handle) {
    if let Some(end_function) = worker_interface.end_run {
        unsafe { end_function(handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut message = pop_message(&mut receiver);
        let body = &message.body[..message.size];
        let message_body = str::from_utf8(&body).unwrap();
        assert_eq!(sentence_to_transfer, message_body);
    }
}
