use std::collections::VecDeque;

use rquickjs::prelude::This;
use rquickjs::{
    class::{OwnedBorrowMut, Trace},
    methods,
    prelude::Opt,
    Class, Ctx, Exception, Function, Promise, Result, Value,
};

use super::{
    downgrade_owned_borrow_mut, promise_rejected_with, ReadableStream, ReadableStreamGenericReader,
    ReadableStreamReadRequest, ReadableStreamReadResult, ReadableStreamReader,
    ReadableStreamReaderOwnedBorrowMut, ReadableStreamState,
};

#[derive(Trace)]
#[rquickjs::class]
pub(crate) struct ReadableStreamDefaultReader<'js> {
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
        ctx: &Ctx<'js>,
        reader: OwnedBorrowMut<'js, Self>,
        // Let stream be reader.[[stream]].
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
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
                let controller = stream
                    .controller
                    .as_ref()
                    .expect("ReadableStreamDefaultReaderRead called without stream controller")
                    .clone();

                controller.pull_steps(
                    ctx,
                    ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(reader),
                    stream,
                    read_request,
                )?;
            },
        }

        Ok(())
    }

    fn set_up_readable_stream_default_reader(
        ctx: &Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<Class<'js, Self>> {
        // If ! IsReadableStreamLocked(stream) is true, throw a TypeError exception.
        if stream.is_readable_stream_locked() {
            return Err(Exception::throw_type(
                ctx,
                "This stream has already been locked for exclusive reading by another reader",
            ));
        }

        // Perform ! ReadableStreamReaderGenericInitialize(reader, stream).
        let generic = ReadableStreamGenericReader::readable_stream_reader_generic_initialize(
            ctx,
            downgrade_owned_borrow_mut(stream),
        )?;
        let mut stream = OwnedBorrowMut::from_class(generic.stream.clone().unwrap());

        let reader = Class::instance(
            ctx.clone(),
            Self {
                generic,
                // Set reader.[[readRequests]] to a new empty list.
                read_requests: VecDeque::new(),
            },
        )?;

        stream.reader = Some(ReadableStreamReader::ReadableStreamDefaultReader(
            reader.clone(),
        ));

        Ok(reader)
    }

    fn readable_stream_default_reader_release(&mut self, ctx: &Ctx<'js>) -> Result<()> {
        // Perform ! ReadableStreamReaderGenericRelease(reader).
        self.generic.readable_stream_reader_generic_release(ctx)?;

        // Let e be a new TypeError exception.
        let e: Value = ctx.eval(r#"new TypeError("Reader was released")"#)?;

        // Perform ! ReadableStreamDefaultReaderErrorReadRequests(reader, e).
        self.readable_stream_default_reader_error_read_requests(e)
    }
}

#[methods(rename_all = "camelCase")]
impl<'js> ReadableStreamDefaultReader<'js> {
    #[qjs(constructor)]
    pub fn new(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<Class<'js, Self>> {
        // Perform ? SetUpReadableStreamDefaultReader(this, stream).
        Self::set_up_readable_stream_default_reader(&ctx, stream)
    }

    fn read(ctx: Ctx<'js>, reader: This<OwnedBorrowMut<'js, Self>>) -> Result<Promise<'js>> {
        let stream = if let Some(ref stream) = reader.generic.stream {
            OwnedBorrowMut::from_class(stream.clone())
        } else {
            // If this.[[stream]] is undefined, return a promise rejected with a TypeError exception.
            let e: Value =
                ctx.eval(r#"new TypeError("Cannot read from a stream using a released reader")"#)?;
            return promise_rejected_with(&ctx, e);
        };

        // Let promise be a new promise.
        let (promise, resolve, reject) = Promise::new(&ctx)?;

        // Let readRequest be a new read request with the following items:
        let read_request = ReadableStreamReadRequest {
            // chunk steps, given chunk
            // Resolve promise with «[ "value" → chunk, "done" → false ]».
            chunk_steps: Function::new(ctx.clone(), {
                let resolve = resolve.clone();
                move |chunk: Value<'js>| -> Result<()> {
                    resolve.call((ReadableStreamReadResult {
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
                    resolve.call((ReadableStreamReadResult {
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
        Self::readable_stream_default_reader_read(&ctx, reader.0, stream, read_request)?;

        // Return promise.
        Ok(promise)
    }

    fn release_lock(&mut self, ctx: Ctx<'js>) -> Result<()> {
        // If this.[[stream]] is undefined, return.
        if self.generic.stream.is_none() {
            return Ok(());
        }

        // Perform ! ReadableStreamDefaultReaderRelease(this).
        self.readable_stream_default_reader_release(&ctx)
    }

    #[qjs(get)]
    fn closed(&self) -> Promise<'js> {
        self.generic.closed_promise.clone()
    }

    fn cancel(
        ctx: Ctx<'js>,
        reader: This<OwnedBorrowMut<'js, Self>>,
        reason: Opt<Value<'js>>,
    ) -> Result<Promise<'js>> {
        // If this.[[stream]] is undefined, return a promise rejected with a TypeError exception.
        if reader.generic.stream.is_none() {
            let e: Value =
                ctx.eval(r#"new TypeError("Cannot cancel a stream using a released reader")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        // Return ! ReadableStreamReaderGenericCancel(this, reason).
        ReadableStreamGenericReader::readable_stream_reader_generic_cancel(
            ctx.clone(),
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(reader.0),
            reason.0.unwrap_or(Value::new_undefined(ctx)),
        )
    }
}
