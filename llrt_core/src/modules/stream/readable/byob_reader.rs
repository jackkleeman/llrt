use rquickjs::{class::Trace, Class, Function, Result, Value};
use std::collections::VecDeque;

use super::ReadableStreamGenericReader;

#[derive(Trace)]
#[rquickjs::class]
pub(super) struct ReadableStreamBYOBReader<'js> {
    pub(super) generic: ReadableStreamGenericReader<'js>,
    pub(super) read_into_requests: VecDeque<ReadableStreamReadIntoRequest<'js>>,
}

impl<'js> ReadableStreamBYOBReader<'js> {
    pub(super) fn readable_stream_byob_reader_error_read_into_requests(
        reader: Class<'js, Self>,
        e: Value<'js>,
    ) -> Result<()> {
        // Let readIntoRequests be reader.[[readIntoRequests]].
        let read_into_requests = &mut reader.borrow_mut().read_into_requests;

        // Set reader.[[readIntoRequests]] to a new empty list.
        let read_into_requests = read_into_requests.split_off(0);
        // For each readIntoRequest of readIntoRequests,
        for read_into_request in read_into_requests {
            // Perform readIntoRequest’s error steps, given e.
            read_into_request.error_steps.call((e.clone(),))?;
        }

        Ok(())
    }
}

#[derive(Trace)]
pub(super) struct ReadableStreamReadIntoRequest<'js> {
    pub(super) chunk_steps: Function<'js>,
    pub(super) close_steps: Function<'js>,
    error_steps: Function<'js>,
}
