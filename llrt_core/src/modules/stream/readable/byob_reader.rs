use llrt_utils::bytes::ObjectBytes;
use rquickjs::{
    class::{OwnedBorrowMut, Trace, Tracer},
    methods,
    prelude::This,
    Class, Ctx, Error, Exception, FromJs, Promise, Result, Value,
};
use std::collections::VecDeque;

use super::{
    byte_controller::ReadableStreamByteController, downgrade_owned_borrow_mut,
    promise_rejected_with, ObjectExt, ReadableStream, ReadableStreamController,
    ReadableStreamGenericReader, ReadableStreamReadResult, ReadableStreamReader,
    ReadableStreamReaderOwnedBorrowMut, ReadableStreamState,
};

#[derive(Trace)]
#[rquickjs::class]
pub(crate) struct ReadableStreamBYOBReader<'js> {
    pub(super) generic: ReadableStreamGenericReader<'js>,
    pub(super) read_into_requests: VecDeque<ReadableStreamReadIntoRequest<'js>>,
}

impl<'js> ReadableStreamBYOBReader<'js> {
    pub(super) fn readable_stream_byob_reader_error_read_into_requests(
        &mut self,
        e: Value<'js>,
    ) -> Result<()> {
        // Let readIntoRequests be reader.[[readIntoRequests]].
        let read_into_requests = &mut self.read_into_requests;

        // Set reader.[[readIntoRequests]] to a new empty list.
        let read_into_requests = read_into_requests.split_off(0);
        // For each readIntoRequest of readIntoRequests,
        for read_into_request in read_into_requests {
            // Perform readIntoRequest’s error steps, given e.
            read_into_request.error_steps(e.clone())?;
        }

        Ok(())
    }

    fn set_up_readable_stream_byob_reader(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<Class<'js, Self>> {
        // If ! IsReadableStreamLocked(stream) is true, throw a TypeError exception.
        if stream.is_readable_stream_locked() {
            return Err(Exception::throw_type(
                &ctx,
                "This stream has already been locked for exclusive reading by another reader",
            ));
        }

        // If stream.[[controller]] does not implement ReadableByteStreamController, throw a TypeError exception.
        match stream.controller {
            Some(ReadableStreamController::ReadableStreamByteController(_)) => {},
            _ => {
                return Err(Exception::throw_type(
                    &ctx,
                    "Cannot construct a ReadableStreamBYOBReader for a stream not constructed with a byte source",
                ));
            },
        };

        // Perform ! ReadableStreamReaderGenericInitialize(reader, stream).
        let generic = ReadableStreamGenericReader::readable_stream_reader_generic_initialize(
            &ctx,
            downgrade_owned_borrow_mut(stream),
        )?;

        let mut stream = OwnedBorrowMut::from_class(generic.stream.clone().unwrap());

        let reader = Class::instance(
            ctx.clone(),
            Self {
                generic,
                // Set reader.[[readIntoRequests]] to a new empty list.
                read_into_requests: VecDeque::new(),
            },
        )?;

        stream.reader = Some(ReadableStreamReader::ReadableStreamBYOBReader(
            reader.clone(),
        ));

        Ok(reader)
    }

    fn readable_stream_byob_reader_release(&mut self, ctx: &Ctx<'js>) -> Result<()> {
        // Perform ! ReadableStreamReaderGenericRelease(reader).
        self.generic.readable_stream_reader_generic_release(ctx)?;

        // Let e be a new TypeError exception.
        let e: Value = ctx.eval(r#"new TypeError("Reader was released")"#)?;
        // Perform ! ReadableStreamBYOBReaderErrorReadIntoRequests(reader, e).
        self.readable_stream_byob_reader_error_read_into_requests(e)
    }

    fn readable_stream_byob_reader_read(
        ctx: &Ctx<'js>,
        reader: OwnedBorrowMut<'js, Self>,
        view: ObjectBytes<'js>,
        min: usize,
        read_into_request: ReadableStreamReadIntoRequest<'js>,
    ) -> Result<()> {
        // Let stream be reader.[[stream]].
        // Assert: stream is not undefined.
        let mut stream = OwnedBorrowMut::from_class(
            reader
                .generic
                .stream
                .clone()
                .expect("ReadableStreamBYOBReaderRead called without stream"),
        );

        // Set stream.[[disturbed]] to true.
        stream.disturbed = true;

        // If stream.[[state]] is "errored", perform readIntoRequest’s error steps given stream.[[storedError]].
        if let ReadableStreamState::Errored = stream.state {
            read_into_request.error_steps(
                stream
                    .stored_error
                    .clone()
                    .expect("stream in error state without stored error")
                    .clone(),
            )
        } else {
            // Otherwise, perform ! ReadableByteStreamControllerPullInto(stream.[[controller]], view, min, readIntoRequest).
            match &stream.controller {
                Some(ReadableStreamController::ReadableStreamByteController(c)) => {
                    let c = OwnedBorrowMut::from_class(c.clone());
                    ReadableStreamByteController::readable_byte_stream_controller_pull_into(
                        ctx,
                        c,
                        stream,
                        Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(reader)),
                        view,
                        min,
                        read_into_request,
                    )
                },
                _ => {
                    panic!(
                        "ReadableStreamBYOBReaderRead called without ReadableStreamByteController"
                    )
                },
            }
        }
    }
}

#[methods(rename_all = "camelCase")]
impl<'js> ReadableStreamBYOBReader<'js> {
    #[qjs(constructor)]
    pub fn new(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<Class<'js, Self>> {
        // Perform ? SetUpReadableStreamBYOBReader(this, stream).
        Self::set_up_readable_stream_byob_reader(ctx, stream)
    }

    fn read(
        ctx: Ctx<'js>,
        reader: This<OwnedBorrowMut<'js, Self>>,
        view: ObjectBytes<'js>,
        options: Option<ReadableStreamBYOBReaderReadOptions>,
    ) -> Result<Promise<'js>> {
        let (buffer, byte_length, _) = view.get_array_buffer()?.unwrap();
        // If view.[[ByteLength]] is 0, return a promise rejected with a TypeError exception.
        if byte_length == 0 {
            let e: Value = ctx.eval(r#"new TypeError("view must have non-zero byteLength")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        // If view.[[ViewedArrayBuffer]].[[ArrayBufferByteLength]] is 0, return a promise rejected with a TypeError exception.
        if buffer.is_empty() {
            let e: Value =
                ctx.eval(r#"new TypeError("view's buffer must have non-zero byteLength")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        // If ! IsDetachedBuffer(view.[[ViewedArrayBuffer]]) is true, return a promise rejected with a TypeError exception.
        if buffer.as_bytes().is_none() {
            let e: Value = ctx.eval(r#"new TypeError("view's buffer has been detached")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        let options = options.unwrap_or(ReadableStreamBYOBReaderReadOptions { min: 1 });

        // If options["min"] is 0, return a promise rejected with a TypeError exception.
        if options.min == 0 {
            let e: Value = ctx.eval(r#"new TypeError("options.min must be greater than 0")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        // If view has a [[TypedArrayName]] internal slot,
        let typed_array_len = match &view {
            ObjectBytes::U8Array(a) => Some(a.len()),
            ObjectBytes::I8Array(a) => Some(a.len()),
            ObjectBytes::U16Array(a) => Some(a.len()),
            ObjectBytes::I16Array(a) => Some(a.len()),
            ObjectBytes::U32Array(a) => Some(a.len()),
            ObjectBytes::I32Array(a) => Some(a.len()),
            ObjectBytes::U64Array(a) => Some(a.len()),
            ObjectBytes::I64Array(a) => Some(a.len()),
            ObjectBytes::F32Array(a) => Some(a.len()),
            ObjectBytes::F64Array(a) => Some(a.len()),
            _ => None,
        };
        if let Some(typed_array_len) = typed_array_len {
            // If options["min"] > view.[[ArrayLength]], return a promise rejected with a RangeError exception.
            if options.min > typed_array_len {
                let e: Value = ctx.eval(
                    r#"new TypeError("options.min must be less than or equal to views length")"#,
                )?;
                return promise_rejected_with(&ctx, e);
            }
        } else {
            // Otherwise (i.e., it is a DataView),
            if options.min > byte_length {
                let e: Value = ctx.eval(
                    r#"new TypeError("options.min must be less than or equal to views byteLength")"#,
                )?;
                return promise_rejected_with(&ctx, e);
            }
        }

        // If this.[[stream]] is undefined, return a promise rejected with a TypeError exception.
        if reader.generic.stream.is_none() {
            let e: Value =
                ctx.eval(r#"new TypeError("Cannot read a stream using a released reader")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        // Let promise be a new promise.
        let (promise, resolve, reject) = Promise::new(&ctx)?;
        // Let readIntoRequest be a new read-into request with the following items:
        let read_into_request = ReadableStreamReadIntoRequest {
            // chunk steps, given chunk
            // Resolve promise with «[ "value" → chunk, "done" → false ]».
            chunk_steps: {
                let resolve = resolve.clone();
                Box::new(move |chunk: Value<'js>| -> Result<()> {
                    resolve.call((ReadableStreamReadResult {
                        value: Some(chunk),
                        done: false,
                    },))
                })
            },
            // close steps, given chunk
            // Resolve promise with «[ "value" → chunk, "done" → true ]».
            close_steps: {
                let resolve = resolve.clone();
                Box::new(move |chunk: Value<'js>| -> Result<()> {
                    resolve.call((ReadableStreamReadResult {
                        value: Some(chunk),
                        done: true,
                    },))
                })
            },
            // error steps, given e
            // Reject promise with e.
            error_steps: {
                let reject = reject.clone();
                Box::new(move |e: Value<'js>| -> Result<()> { reject.call((e,)) })
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

        // Perform ! ReadableStreamBYOBReaderRead(this, view, options["min"], readIntoRequest).
        Self::readable_stream_byob_reader_read(
            &ctx,
            reader.0,
            view,
            options.min,
            read_into_request,
        )?;

        // Return promise.
        Ok(promise)
    }

    fn release_lock(&mut self, ctx: Ctx<'js>) -> Result<()> {
        // If this.[[stream]] is undefined, return.
        if self.generic.stream.is_none() {
            return Ok(());
        }

        // Perform ! ReadableStreamBYOBReaderRelease(this).
        self.readable_stream_byob_reader_release(&ctx)
    }
}

struct ReadableStreamBYOBReaderReadOptions {
    min: usize,
}

impl<'js> FromJs<'js> for ReadableStreamBYOBReaderReadOptions {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let min = obj.get_optional::<_, _>("min")?.unwrap_or(1);

        Ok(Self { min })
    }
}

pub(super) struct ReadableStreamReadIntoRequest<'js> {
    chunk_steps: Box<dyn FnOnce(Value<'js>) -> Result<()> + 'js>,
    close_steps: Box<dyn FnOnce(Value<'js>) -> Result<()> + 'js>,
    error_steps: Box<dyn FnOnce(Value<'js>) -> Result<()> + 'js>,
    trace: Box<dyn Fn(Tracer<'_, 'js>) + 'js>,
}

impl<'js> ReadableStreamReadIntoRequest<'js> {
    pub(super) fn chunk_steps(self, chunk: Value<'js>) -> Result<()> {
        let chunk_steps = self.chunk_steps;
        chunk_steps(chunk)
    }

    pub(super) fn close_steps(self, chunk: Value<'js>) -> Result<()> {
        let close_steps = self.close_steps;
        close_steps(chunk)
    }

    pub(super) fn error_steps(self, reason: Value<'js>) -> Result<()> {
        let error_steps = self.error_steps;
        error_steps(reason)
    }
}

impl<'js> Trace<'js> for ReadableStreamReadIntoRequest<'js> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        (self.trace)(tracer)
    }
}
