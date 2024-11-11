use llrt_utils::bytes::ObjectBytes;
use rquickjs::{
    atom::PredefinedAtom,
    class::{JsClass, OwnedBorrowMut, Trace, Tracer},
    function::Constructor,
    methods,
    prelude::{Opt, This},
    ArrayBuffer, Class, Ctx, Error, Exception, FromJs, Function, IntoJs, Object, Promise, Result,
    Value,
};
use std::collections::VecDeque;

use crate::modules::stream::downgrade_owned_borrow_mut;

use super::{
    byte_controller::ReadableByteStreamController, promise_rejected_with, ObjectExt,
    ReadableStream, ReadableStreamController, ReadableStreamControllerOwnedBorrowMut,
    ReadableStreamGenericReader, ReadableStreamReadResult, ReadableStreamReader,
    ReadableStreamState,
};

#[derive(Trace)]
#[rquickjs::class]
pub(crate) struct ReadableStreamBYOBReader<'js> {
    pub(super) generic: ReadableStreamGenericReader<'js>,
    pub(super) read_into_requests: VecDeque<ReadableStreamReadIntoRequest<'js>>,
}

impl<'js> ReadableStreamBYOBReader<'js> {
    pub(super) fn readable_stream_byob_reader_error_read_into_requests(
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        mut reader: OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
        e: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
    )> {
        // Let readIntoRequests be reader.[[readIntoRequests]].
        let read_into_requests = &mut reader.read_into_requests;

        // Set reader.[[readIntoRequests]] to a new empty list.
        let read_into_requests = read_into_requests.split_off(0);
        // For each readIntoRequest of readIntoRequests,
        for read_into_request in read_into_requests {
            // Perform readIntoRequest’s error steps, given e.
            (stream, controller, reader) =
                read_into_request.error_steps(stream, controller, reader, e.clone())?;
        }

        Ok((stream, controller, reader))
    }

    pub(super) fn set_up_readable_stream_byob_reader(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<(OwnedBorrowMut<'js, ReadableStream<'js>>, Class<'js, Self>)> {
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

        Ok((stream, reader))
    }

    pub(super) fn readable_stream_byob_reader_release(
        ctx: &Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        mut reader: OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
    )> {
        // Perform ! ReadableStreamReaderGenericRelease(reader).
        reader
            .generic
            .readable_stream_reader_generic_release(ctx, &mut stream, || {
                controller.release_steps()
            })?;

        // Let e be a new TypeError exception.
        let e: Value = ctx.eval(r#"new TypeError("Reader was released")"#)?;
        // Perform ! ReadableStreamBYOBReaderErrorReadIntoRequests(reader, e).
        (stream, controller, _) = Self::readable_stream_byob_reader_error_read_into_requests(
            stream, controller, reader, e,
        )?;
        Ok((stream, controller))
    }

    pub(super) fn readable_stream_byob_reader_read(
        ctx: &Ctx<'js>,
        // Let stream be reader.[[stream]].
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        reader: OwnedBorrowMut<'js, Self>,
        view: ViewBytes<'js>,
        min: u64,
        read_into_request: ReadableStreamReadIntoRequest<'js>,
    ) -> Result<()> {
        // Set stream.[[disturbed]] to true.
        stream.disturbed = true;

        // If stream.[[state]] is "errored", perform readIntoRequest’s error steps given stream.[[storedError]].
        if let ReadableStreamState::Errored = stream.state {
            let stored_error = stream
                .stored_error
                .clone()
                .expect("stream in error state without stored error")
                .clone();
            read_into_request.error_steps(stream, controller, reader, stored_error)?;
        } else {
            // Otherwise, perform ! ReadableByteStreamControllerPullInto(stream.[[controller]], view, min, readIntoRequest).
            ReadableByteStreamController::readable_byte_stream_controller_pull_into(
                ctx,
                controller,
                stream,
                reader,
                view,
                min,
                read_into_request,
            )?;
        }
        Ok(())
    }
}

#[methods(rename_all = "camelCase")]
impl<'js> ReadableStreamBYOBReader<'js> {
    // this is required by web platform tests
    #[qjs(get)]
    pub fn constructor(ctx: Ctx<'js>) -> Result<Option<Constructor>> {
        <ReadableStreamBYOBReader as JsClass>::constructor(&ctx)
    }

    #[qjs(constructor)]
    pub fn new(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<Class<'js, Self>> {
        // Perform ? SetUpReadableStreamBYOBReader(this, stream).
        let (_, reader) = Self::set_up_readable_stream_byob_reader(ctx, stream)?;
        Ok(reader)
    }

    fn read(
        ctx: Ctx<'js>,
        reader: This<OwnedBorrowMut<'js, Self>>,
        view: Opt<Value<'js>>,
        options: Opt<Value<'js>>,
    ) -> Result<Promise<'js>> {
        let options = match options.0 {
            None => ReadableStreamBYOBReaderReadOptions { min: 1 },
            Some(value) => match ReadableStreamBYOBReaderReadOptions::from_js(&ctx, value) {
                Ok(value) => value,
                Err(Error::Exception) => {
                    return promise_rejected_with(&ctx, ctx.catch());
                },
                Err(err) => return Err(err),
            },
        };

        let view = view.0.unwrap_or_else(|| Value::new_undefined(ctx.clone()));
        let view = match ViewBytes::from_js(&ctx, view) {
            Ok(view) => view,
            Err(Error::Exception) => {
                return promise_rejected_with(&ctx, ctx.catch());
            },
            Err(err) => return Err(err),
        };

        let (buffer, byte_length) = match view.get_array_buffer() {
            Ok((buffer, byte_length, _)) => (buffer, byte_length),
            // this can happen if its detached
            Err(Error::Exception) => return promise_rejected_with(&ctx, ctx.catch()),
            Err(err) => return Err(err),
        };

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

        // If options["min"] is 0, return a promise rejected with a TypeError exception.
        if options.min == 0 {
            let e: Value = ctx.eval(r#"new TypeError("options.min must be greater than 0")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        // If view has a [[TypedArrayName]] internal slot,
        let typed_array_len = match &view.0 {
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
            if options.min > typed_array_len as u64 {
                let e: Value = ctx.eval(
                    r#"new RangeError("options.min must be less than or equal to views length")"#,
                )?;
                return promise_rejected_with(&ctx, e);
            }
        } else {
            // Otherwise (i.e., it is a DataView),
            // If options["min"] > view.[[ByteLength]], return a promise rejected with a RangeError exception.
            if options.min > byte_length as u64 {
                let e: Value = ctx.eval(
                    r#"new RangeError("options.min must be less than or equal to views byteLength")"#,
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
                Box::new(move |stream, controller, reader, chunk: Value<'js>| {
                    let () = resolve.call((ReadableStreamReadResult {
                        value: Some(chunk),
                        done: false,
                    },))?;
                    Ok((stream, controller, reader))
                })
            },
            // close steps, given chunk
            // Resolve promise with «[ "value" → chunk, "done" → true ]».
            close_steps: {
                let resolve = resolve.clone();
                Box::new(move |stream, controller, reader, chunk: Value<'js>| {
                    let () = resolve.call((ReadableStreamReadResult {
                        value: Some(chunk),
                        done: true,
                    },))?;
                    Ok((stream, controller, reader))
                })
            },
            // error steps, given e
            // Reject promise with e.
            error_steps: {
                let reject = reject.clone();
                Box::new(move |stream, controller, reader, e: Value<'js>| {
                    let () = reject.call((e,))?;
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

        let stream = OwnedBorrowMut::from_class(
            reader
                .generic
                .stream
                .clone()
                .expect("ReadableStreamBYOBReader read called without stream"),
        );

        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(
            stream
                .controller
                .clone()
                .expect("ReadableStreamBYOBReader read called without controller"),
        )
        .into_byte_controller()
        .expect("ReadableStreamBYOBReader read called without byte controller");

        // Perform ! ReadableStreamBYOBReaderRead(this, view, options["min"], readIntoRequest).
        Self::readable_stream_byob_reader_read(
            &ctx,
            stream,
            controller,
            reader.0,
            view,
            options.min,
            read_into_request,
        )?;

        // Return promise.
        Ok(promise)
    }

    fn release_lock(ctx: Ctx<'js>, reader: This<OwnedBorrowMut<'js, Self>>) -> Result<()> {
        // If this.[[stream]] is undefined, return.
        let stream = match reader.generic.stream.clone() {
            None => {
                return Ok(());
            },
            Some(stream) => OwnedBorrowMut::from_class(stream),
        };

        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(
            stream
                .controller
                .clone()
                .expect("releaseLock called on byob reader without controller"),
        )
        .into_byte_controller()
        .expect("releaseLock called on byob reader with non-byte controller");

        // Perform ! ReadableStreamBYOBReaderRelease(this).
        Self::readable_stream_byob_reader_release(&ctx, stream, controller, reader.0)?;
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

struct ReadableStreamBYOBReaderReadOptions {
    min: u64,
}

impl<'js> FromJs<'js> for ReadableStreamBYOBReaderReadOptions {
    fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let min = obj.get_optional::<_, f64>("min")?.unwrap_or(1.0);
        if min < u64::MIN as f64 || min > u64::MAX as f64 {
            return Err(Exception::throw_type(
                ctx,
                "min on ReadableStreamBYOBReaderReadOptions must fit into unsigned long long",
            ));
        };

        Ok(Self { min: min as u64 })
    }
}

pub(super) struct ReadableStreamReadIntoRequest<'js> {
    pub(super) chunk_steps: Box<
        dyn FnOnce(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
                OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
                Value<'js>,
            ) -> Result<(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
                OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
            )> + 'js,
    >,
    pub(super) close_steps: Box<
        dyn FnOnce(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
                OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
                Value<'js>,
            ) -> Result<(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
                OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
            )> + 'js,
    >,
    pub(super) error_steps: Box<
        dyn FnOnce(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
                OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
                Value<'js>,
            ) -> Result<(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
                OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
            )> + 'js,
    >,
    pub(super) trace: Box<dyn Fn(Tracer<'_, 'js>) + 'js>,
}

impl<'js> ReadableStreamReadIntoRequest<'js> {
    pub(super) fn chunk_steps(
        self,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        reader: OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
        chunk: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
    )> {
        let chunk_steps = self.chunk_steps;
        chunk_steps(stream, controller, reader, chunk)
    }

    pub(super) fn close_steps(
        self,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        reader: OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
        chunk: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
    )> {
        let close_steps = self.close_steps;
        close_steps(stream, controller, reader, chunk)
    }

    pub(super) fn error_steps(
        self,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        reader: OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
        reason: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>,
    )> {
        let error_steps = self.error_steps;
        error_steps(stream, controller, reader, reason)
    }
}

impl<'js> Trace<'js> for ReadableStreamReadIntoRequest<'js> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        (self.trace)(tracer)
    }
}

#[derive(Clone)]
pub(super) struct ViewBytes<'js>(ObjectBytes<'js>);

impl<'js> ViewBytes<'js> {
    pub(super) fn from_object(ctx: &Ctx<'js>, object: &Object<'js>) -> Result<Self> {
        let ab = ctx
            .globals()
            .get::<_, Object>(rquickjs::atom::PredefinedAtom::ArrayBuffer)?;
        if ab
            .get::<_, Function>("isView")?
            .call::<_, bool>((object.clone(),))?
        {
            if let Some(view) = ObjectBytes::from_array_buffer(object)? {
                return Ok(Self(view));
            }
        }

        Err(Exception::throw_type(
            ctx,
            "view must be an ArrayBufferView",
        ))
    }

    pub(super) fn get_array_buffer(&self) -> Result<(ArrayBuffer<'js>, usize, usize)> {
        Ok(self
            .0
            .get_array_buffer()?
            .expect("invariant broken; ViewBytes may not contain ObjectBytes::Vec"))
    }

    pub(super) fn element_size(&self) -> usize {
        match self.0 {
            ObjectBytes::U8Array(_) => 1,
            ObjectBytes::I8Array(_) => 1,
            ObjectBytes::U16Array(_) => 2,
            ObjectBytes::I16Array(_) => 2,
            ObjectBytes::U32Array(_) => 4,
            ObjectBytes::I32Array(_) => 4,
            ObjectBytes::U64Array(_) => 8,
            ObjectBytes::I64Array(_) => 8,
            ObjectBytes::F32Array(_) => 4,
            ObjectBytes::F64Array(_) => 8,
            ObjectBytes::DataView(_) => 1,
            ObjectBytes::Vec(_) => {
                panic!("invariant broken; ViewBytes may not contain ObjectBytes::Vec")
            },
        }
    }

    pub(super) fn atom(&self) -> PredefinedAtom {
        match self.0 {
            ObjectBytes::U8Array(_) => PredefinedAtom::Uint8Array,
            ObjectBytes::I8Array(_) => PredefinedAtom::Int8Array,
            ObjectBytes::U16Array(_) => PredefinedAtom::Uint16Array,
            ObjectBytes::I16Array(_) => PredefinedAtom::Int16Array,
            ObjectBytes::U32Array(_) => PredefinedAtom::Uint32Array,
            ObjectBytes::I32Array(_) => PredefinedAtom::Int32Array,
            ObjectBytes::U64Array(_) => PredefinedAtom::BigUint64Array,
            ObjectBytes::I64Array(_) => PredefinedAtom::BigInt64Array,
            ObjectBytes::F32Array(_) => PredefinedAtom::Float32Array,
            ObjectBytes::F64Array(_) => PredefinedAtom::Float64Array,
            ObjectBytes::DataView(_) => PredefinedAtom::DataView,
            ObjectBytes::Vec(_) => {
                panic!("invariant broken; ViewBytes may not contain ObjectBytes::Vec")
            },
        }
    }
}

impl<'js> FromJs<'js> for ViewBytes<'js> {
    fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        match value.as_object() {
            None => {
                Err(Exception::throw_type(
                    ctx,
                    "view must be typed DataView, Buffer, ArrayBuffer, or Uint8Array, but is not an object",
                ))
            },
            Some(object) => Self::from_object(ctx, object),
        }
    }
}

impl<'js> Trace<'js> for ViewBytes<'js> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        self.0.trace(tracer);
    }
}

impl<'js> IntoJs<'js> for ViewBytes<'js> {
    fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
        self.0.into_js(ctx)
    }
}
