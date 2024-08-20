use std::collections::VecDeque;

use rquickjs::{
    class::Trace, methods, Class, Ctx, Exception, Function, IntoJs, Object, Promise,
    Result, Value,
};

use super::{
    ReadableStream, ReadableStreamGenericReader, ReadableStreamReadRequest, ReadableStreamState,
};

#[derive(Trace)]
#[rquickjs::class]
pub(super) struct ReadableStreamDefaultReader<'js> {
    pub(super) generic: ReadableStreamGenericReader<'js>,
    pub(super) read_requests: VecDeque<ReadableStreamReadRequest<'js>>,
}

impl<'js> ReadableStreamDefaultReader<'js> {
    pub(super) fn readable_stream_default_reader_error_read_requests(
        &mut self,
        e: Value<'js>,
    ) -> Result<()> {
        // Let readRequests be reader.[[readRequests]].
        let read_requests = &mut self.read_requests;

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
        &self,
        ctx: &Ctx<'js>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        // Let stream be reader.[[stream]].
        let stream = self.generic.stream.clone();
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

    fn set_up_readable_stream_default_reader(
        ctx: &Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
    ) -> Result<Class<'js, Self>> {
        // If ! IsReadableStreamLocked(stream) is true, throw a TypeError exception.
        if stream.borrow().is_readable_stream_locked() {
            return Err(Exception::throw_type(
                ctx,
                "This stream has already been locked for exclusive reading by another reader",
            ));
        }

        // Perform ! ReadableStreamReaderGenericInitialize(reader, stream).
        let generic = ReadableStreamGenericReader::readable_stream_generic_initialize(ctx, stream)?;

        Class::instance(
            ctx.clone(),
            Self {
                generic,
                // Set reader.[[readRequests]] to a new empty list.
                read_requests: VecDeque::new(),
            },
        )
    }
}

#[methods]
impl<'js> ReadableStreamDefaultReader<'js> {
    #[qjs(constructor)]
    fn new(ctx: Ctx<'js>, stream: Class<'js, ReadableStream<'js>>) -> Result<Class<'js, Self>> {
        // Perform ? SetUpReadableStreamDefaultReader(this, stream).
        Self::set_up_readable_stream_default_reader(&ctx, stream)
    }

    fn read(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
        // If this.[[stream]] is undefined, return a promise rejected with a TypeError exception.
        // this is not currently possible in this type system

        // Let promise be a new promise.
        let (promise, resolve, reject) = Promise::new(&ctx)?;

        // Let readRequest be a new read request with the following items:
        let read_request = ReadableStreamReadRequest {
            // chunk steps, given chunk
            // Resolve promise with «[ "value" → chunk, "done" → false ]».
            chunk_steps: Function::new(ctx.clone(), {
                let resolve = resolve.clone();
                move |chunk: Value<'js>| -> Result<()> {
                    resolve.call((ReadResult {
                        value: chunk,
                        done: false,
                    },))
                }
            })?,
            // close steps
            // Resolve promise with «[ "value" → undefined, "done" → true ]».
            close_steps: Function::new(ctx.clone(), {
                let resolve = resolve.clone();
                let ctx = ctx.clone();
                move || -> Result<()> {
                    resolve.call((ReadResult {
                        value: Value::new_undefined(ctx.clone()),
                        done: true,
                    },))
                }
            })?,
            // error steps, given e
            // Reject promise with e.
            error_steps: Function::new(ctx.clone(), {
                let reject = reject.clone();
                move |e: Value<'js>| -> Result<()> { reject.call((e,)) }
            })?,
        };

        // Perform ! ReadableStreamDefaultReaderRead(this, readRequest).
        self.readable_stream_default_reader_read(&ctx, read_request)?;

        // Return promise.
        Ok(promise)
    }
}

struct ReadResult<'js> {
    value: Value<'js>,
    done: bool,
}

impl<'js> IntoJs<'js> for ReadResult<'js> {
    fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
        let obj = Object::new(ctx.clone())?;
        obj.set("value", self.value)?;
        obj.set("done", self.done)?;
        Ok(obj.into_value())
    }
}
