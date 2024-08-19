use std::collections::VecDeque;

use rquickjs::{class::Trace, Class, Ctx, Result, Value};

use super::{ReadableStreamGenericReader, ReadableStreamReadRequest, ReadableStreamState};

#[derive(Trace)]
#[rquickjs::class]
pub(super) struct ReadableStreamDefaultReader<'js> {
    pub(super) generic: ReadableStreamGenericReader<'js>,
    pub(super) read_requests: VecDeque<ReadableStreamReadRequest<'js>>,
}

impl<'js> ReadableStreamDefaultReader<'js> {
    pub(super) fn readable_stream_default_reader_error_read_requests(
        reader: Class<'js, Self>,
        e: Value<'js>,
    ) -> Result<()> {
        // Let readRequests be reader.[[readRequests]].
        let read_requests = &mut reader.borrow_mut().read_requests;

        // Set reader.[[readRequests]] to a new empty list.
        let read_requests = read_requests.split_off(0);
        // For each readRequest of readRequests,
        for read_request in read_requests {
            // Perform readRequest’s error steps, given e.
            read_request.error_steps.call((e.clone(),))?;
        }

        Ok(())
    }

    fn readable_stream_default_reader_read(
        ctx: &Ctx<'js>,
        reader: Class<'js, Self>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        // Let stream be reader.[[stream]].
        let stream = reader.borrow().generic.stream.clone();
        let mut stream = stream.borrow_mut();
        // Set stream.[[disturbed]] to true.
        stream.disturbed = true;
        match stream.state {
            // If stream.[[state]] is "closed", perform readRequest’s close steps.
            ReadableStreamState::Closed => read_request.close_steps.call(())?,
            // Otherwise, if stream.[[state]] is "errored", perform readRequest’s error steps given stream.[[storedError]].
            ReadableStreamState::Errored => {
                read_request
                    .error_steps
                    .call((stream.stored_error.clone(),))?;
            },
            // Otherwise,
            _ => {
                // Perform ! stream.[[controller]].[[PullSteps]](readRequest).
                stream
                    .controller
                    .as_ref()
                    .expect("ReadableStreamDefaultReaderRead called without stream controller")
                    .pull_steps(ctx, read_request)?;
            },
        }

        Ok(())
    }
}
