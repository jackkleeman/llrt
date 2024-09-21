use std::collections::VecDeque;

use rquickjs::prelude::This;
use rquickjs::{
    class::{OwnedBorrowMut, Trace},
    methods,
    prelude::Opt,
    Class, Ctx, Exception, Promise, Result, Value,
};

use super::{
    downgrade_owned_borrow_mut, promise_rejected_with, ReadableStream,
    ReadableStreamControllerOwnedBorrowMut, ReadableStreamGenericReader, ReadableStreamReadRequest,
    ReadableStreamReadResult, ReadableStreamReader, ReadableStreamState,
};

#[derive(Trace)]
#[rquickjs::class]
pub(crate) struct ReadableStreamDefaultReader<'js> {
    pub(super) generic: ReadableStreamGenericReader<'js>,
    pub(super) read_requests: VecDeque<ReadableStreamReadRequest<'js>>,
}

impl<'js> ReadableStreamDefaultReader<'js> {
    pub(super) fn readable_stream_default_reader_error_read_requests(
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        mut reader: OwnedBorrowMut<'js, Self>,
        e: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        ReadableStreamControllerOwnedBorrowMut<'js>,
        OwnedBorrowMut<'js, Self>,
    )> {
        // Let readRequests be reader.[[readRequests]].
        let read_requests = &mut reader.read_requests;

        // Set reader.[[readRequests]] to a new empty list.
        let read_requests = read_requests.split_off(0);

        let mut reader = Some(reader.into());

        // For each readRequest of readRequests,
        for read_request in read_requests {
            // Perform readRequest’s error steps, given e.
            (stream, controller, reader) =
                read_request.error_steps(stream, controller, reader, e.clone())?;
        }

        let reader = reader
            .and_then(|r| r.into_default_reader())
            .expect("error_steps must return the same type of reader");

        Ok((stream, controller, reader))
    }

    pub(super) fn readable_stream_default_reader_read<'closure>(
        ctx: &Ctx<'js>,
        // Let stream be reader.[[stream]].
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        reader: OwnedBorrowMut<'js, Self>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        // Set stream.[[disturbed]] to true.
        stream.disturbed = true;
        match stream.state {
            // If stream.[[state]] is "closed", perform readRequest’s close steps.
            ReadableStreamState::Closed => {
                read_request.close_steps(ctx, stream, controller, Some(reader.into()))?;
            },
            // Otherwise, if stream.[[state]] is "errored", perform readRequest’s error steps given stream.[[storedError]].
            ReadableStreamState::Errored => {
                let stored_error = stream
                    .stored_error
                    .clone()
                    .expect("stream in error state without stored error");
                read_request.error_steps(stream, controller, Some(reader.into()), stored_error)?;
            },
            // Otherwise,
            _ => {
                // Perform ! stream.[[controller]].[[PullSteps]](readRequest).
                controller.pull_steps(ctx, stream, reader.into(), read_request)?;
            },
        }

        Ok(())
    }

    pub(super) fn set_up_readable_stream_default_reader(
        ctx: &Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<(OwnedBorrowMut<'js, ReadableStream<'js>>, Class<'js, Self>)> {
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

        Ok((stream, reader))
    }

    pub(super) fn readable_stream_default_reader_release(
        ctx: &Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        mut reader: OwnedBorrowMut<'js, Self>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        ReadableStreamControllerOwnedBorrowMut<'js>,
        OwnedBorrowMut<'js, Self>,
    )> {
        // Perform ! ReadableStreamReaderGenericRelease(reader).
        reader
            .generic
            .readable_stream_reader_generic_release(ctx, &mut stream, || {
                controller.release_steps()
            })?;

        // Let e be a new TypeError exception.
        let e: Value = ctx.eval(r#"new TypeError("Reader was released")"#)?;

        // Perform ! ReadableStreamDefaultReaderErrorReadRequests(reader, e).
        Self::readable_stream_default_reader_error_read_requests(stream, controller, reader, e)
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
        let (_, reader) = Self::set_up_readable_stream_default_reader(&ctx, stream)?;
        Ok(reader)
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

        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(
            stream
                .controller
                .clone()
                .expect("ReadableStreamDefaultReader read called without controller"),
        );

        // Let promise be a new promise.
        let (promise, resolve, reject) = Promise::new(&ctx)?;

        // Let readRequest be a new read request with the following items:
        let read_request = ReadableStreamReadRequest {
            // chunk steps, given chunk
            // Resolve promise with «[ "value" → chunk, "done" → false ]».
            chunk_steps: {
                let resolve = resolve.clone();
                Box::new(move |stream, controller, reader, chunk: Value<'js>| {
                    () = resolve.call((ReadableStreamReadResult {
                        value: Some(chunk),
                        done: false,
                    },))?;
                    Ok((stream, controller, reader))
                })
            },
            // close steps
            // Resolve promise with «[ "value" → undefined, "done" → true ]».
            close_steps: {
                let resolve = resolve.clone();
                Box::new(move |_, stream, controller, reader| {
                    resolve.call((ReadableStreamReadResult {
                        value: None,
                        done: true,
                    },))?;
                    Ok((stream, controller, reader))
                })
            },
            // error steps, given e
            // Reject promise with e.
            error_steps: {
                let reject = reject.clone();
                Box::new(move |stream, controller, reader, e| {
                    reject.call((e,))?;
                    Ok((stream, controller, reader))
                })
            },
            trace: {
                let resolve = resolve.clone();
                let reject = reject.clone();
                Box::new(move |tracer| {
                    resolve.trace(tracer);
                    reject.trace(tracer);
                })
            },
        };

        // Perform ! ReadableStreamDefaultReaderRead(this, readRequest).
        Self::readable_stream_default_reader_read(
            &ctx,
            stream,
            controller,
            reader.0,
            read_request,
        )?;

        // Return promise.
        Ok(promise)
    }

    fn release_lock(ctx: Ctx<'js>, reader: This<OwnedBorrowMut<'js, Self>>) -> Result<()> {
        let stream = match reader.generic.stream.clone() {
            // If this.[[stream]] is undefined, return.
            None => {
                return Ok(());
            },
            Some(stream) => OwnedBorrowMut::from_class(stream),
        };

        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(
            stream
                .controller
                .clone()
                .expect("ReadableStreamDefaultReader releaseLock called without controller"),
        );

        // Perform ! ReadableStreamDefaultReaderRelease(this).
        Self::readable_stream_default_reader_release(&ctx, stream, controller, reader.0)?;
        Ok(())
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
        let stream = match reader.generic.stream.clone() {
            // If this.[[stream]] is undefined, return a promise rejected with a TypeError exception.
            None => {
                let e: Value =
                    ctx.eval(r#"new TypeError("Cannot cancel a stream using a released reader")"#)?;
                return promise_rejected_with(&ctx, e);
            },
            Some(stream) => OwnedBorrowMut::from_class(stream),
        };

        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(
            stream
                .controller
                .clone()
                .expect("ReadableStreamDefaultReader cancel called without controller"),
        );

        // Return ! ReadableStreamReaderGenericCancel(this, reason).
        let (promise, _, _, _) =
            ReadableStreamGenericReader::readable_stream_reader_generic_cancel(
                ctx.clone(),
                stream,
                controller,
                reader.0.into(),
                reason.0.unwrap_or(Value::new_undefined(ctx)),
            )?;
        Ok(promise)
    }
}
