use std::{cell::Cell, rc::Rc};

use crate::modules::events::abort_signal::AbortSignal;
use byob_reader::ReadableStreamReadIntoRequest;
use byte_controller::ReadableStreamByteController;
use default_controller::ReadableStreamDefaultController;
use llrt_utils::{
    bytes::ObjectBytes, error_messages::ERROR_MSG_ARRAY_BUFFER_DETACHED, object::ObjectExt,
    result::ResultExt,
};
use rquickjs::{
    class::{JsClass, OwnedBorrow, OwnedBorrowMut, Trace, Tracer},
    prelude::{List, OnceFn, Opt, This},
    ArrayBuffer, Class, Ctx, Error, Exception, FromJs, Function, IntoJs, Object, Promise, Result,
    Type, Undefined, Value,
};

use super::{writeable::WriteableStream, ReadableWritablePair};

mod byob_reader;
mod byte_controller;
mod count_queueing_strategy;
mod default_controller;
mod default_reader;

pub(crate) use byob_reader::ReadableStreamBYOBReader;
pub(crate) use count_queueing_strategy::CountQueuingStrategy;
pub(crate) use default_reader::ReadableStreamDefaultReader;

#[rquickjs::class]
#[derive(Trace)]
pub struct ReadableStream<'js> {
    controller: Option<ReadableStreamController<'js>>,
    disturbed: bool,
    state: ReadableStreamState,
    reader: Option<ReadableStreamReader<'js>>,
    stored_error: Option<Value<'js>>,
}

#[derive(Trace, Clone, Copy, PartialEq, Eq)]
enum ReadableStreamState {
    Readable,
    Closed,
    Errored,
}

#[rquickjs::methods(rename_all = "camelCase")]
impl<'js> ReadableStream<'js> {
    // Streams Spec: 4.2.4: https://streams.spec.whatwg.org/#rs-prototype
    // constructor(optional object underlyingSource, optional QueuingStrategy strategy = {});
    #[qjs(constructor)]
    fn new(
        ctx: Ctx<'js>,
        underlying_source: Opt<Value<'js>>,
        queuing_strategy: Opt<QueuingStrategy<'js>>,
    ) -> Result<Class<'js, Self>> {
        // If underlyingSource is missing, set it to null.
        let underlying_source = match underlying_source.0 {
            None => Value::new_null(ctx.clone()),
            Some(underlying_source) => underlying_source,
        };

        // Let underlyingSourceDict be underlyingSource, converted to an IDL value of type UnderlyingSource.
        let underlying_source_dict: Option<UnderlyingSource<'js>> =
            FromJs::from_js(&ctx, underlying_source.clone())?;

        let stream_class = Class::instance(
            ctx.clone(),
            Self {
                // Set stream.[[state]] to "readable".
                state: ReadableStreamState::Readable,
                // Set stream.[[reader]] and stream.[[storedError]] to undefined.
                reader: None,
                stored_error: None,
                // Set stream.[[disturbed]] to false.
                disturbed: false,
                controller: None,
            },
        )?;
        let stream = OwnedBorrowMut::from_class(stream_class.clone());

        match underlying_source_dict
            .as_ref()
            .and_then(|s| s.r#type.as_ref())
        {
            // If underlyingSourceDict["type"] is "bytes":
            Some(ReadableStreamType::Bytes) => {
                // If strategy["size"] exists, throw a RangeError exception.
                if queuing_strategy
                    .0
                    .as_ref()
                    .and_then(|qs| qs.size.as_ref())
                    .is_some()
                {
                    return Err(Exception::throw_range(
                        &ctx,
                        "The strategy for a byte stream cannot have a size function",
                    ));
                }
                // Let highWaterMark be ? ExtractHighWaterMark(strategy, 0).
                let high_water_mark =
                    QueuingStrategy::extract_high_water_mark(&ctx, &queuing_strategy, 0.0)?
                        as usize;

                // Perform ? SetUpReadableByteStreamControllerFromUnderlyingSource(this, underlyingSource, underlyingSourceDict, highWaterMark).
                byte_controller::ReadableStreamByteController::set_up_readable_byte_stream_controller_from_underlying_source(
                    &ctx,
                    stream,
                    underlying_source,
                    underlying_source_dict,
                    high_water_mark,
                )?;
            },
            // Otherwise,
            None => {
                // Let sizeAlgorithm be ! ExtractSizeAlgorithm(strategy).
                let size_algorithm = QueuingStrategy::extract_size_algorithm(&queuing_strategy);

                // Let highWaterMark be ? ExtractHighWaterMark(strategy, 1).
                let high_water_mark =
                    QueuingStrategy::extract_high_water_mark(&ctx, &queuing_strategy, 1.0)?;

                // Perform ? SetUpReadableStreamDefaultControllerFromUnderlyingSource(this, underlyingSource, underlyingSourceDict, highWaterMark, sizeAlgorithm).
                ReadableStreamDefaultController::set_up_readable_stream_default_controller_from_underlying_source(
                    ctx,
                    stream,
                    underlying_source,
                    underlying_source_dict,
                    high_water_mark,
                    size_algorithm,
                )?;
            },
        }

        Ok(stream_class)
    }

    // static ReadableStream from(any asyncIterable);
    #[qjs(static)]
    fn from(_async_iterable: Value<'js>) -> Class<'js, Self> {
        unimplemented!()
    }

    // readonly attribute boolean locked;
    #[qjs(get)]
    fn locked(&self) -> bool {
        // Return ! IsReadableStreamLocked(this).
        self.is_readable_stream_locked()
    }

    // Promise<undefined> cancel(optional any reason);
    fn cancel(
        ctx: Ctx<'js>,
        mut stream: This<OwnedBorrowMut<'js, Self>>,
        reason: Opt<Value<'js>>,
    ) -> Result<Promise<'js>> {
        // If ! IsReadableStreamLocked(this) is true, return a promise rejected with a TypeError exception.
        if stream.is_readable_stream_locked() {
            let e: Value =
                ctx.eval(r#"new TypeError("Cannot cancel a stream that already has a reader")"#)?;
            return promise_rejected_with(&ctx, e);
        }
        let reader = stream.reader_mut();
        Self::readable_stream_cancel(
            &ctx,
            stream.0,
            reader,
            reason.0.unwrap_or(Value::new_undefined(ctx.clone())),
        )
    }

    // ReadableStreamReader getReader(optional ReadableStreamGetReaderOptions options = {});
    fn get_reader(
        ctx: Ctx<'js>,
        stream: This<OwnedBorrowMut<'js, Self>>,
        options: Opt<ReadableStreamGetReaderOptions>,
    ) -> Result<Value<'js>> {
        let (stream_class, stream) = class_from_owned_borrow_mut(stream.0);
        // If options["mode"] does not exist, return ? AcquireReadableStreamDefaultReader(this).
        match options.0 {
            None | Some(ReadableStreamGetReaderOptions { mode: None }) => {
                ReadableStreamReader::acquire_readable_stream_default_reader(ctx.clone(), stream)?
            },
            // Return ? AcquireReadableStreamBYOBReader(this).
            Some(ReadableStreamGetReaderOptions {
                mode: Some(ReadableStreamReaderMode::Byob),
            }) => ReadableStreamReader::acquire_readable_stream_byob_reader(ctx.clone(), stream)?,
        }

        // tricky dance; OwnedBorrowMut values aren't cloneable but JS values are
        let mut stream = OwnedBorrowMut::from_class(stream_class);
        let reader = stream.reader.take().unwrap();
        let value = reader.into_js(&ctx)?;
        stream.reader = Some(FromJs::from_js(&ctx, value.clone())?);
        Ok(value)
    }

    // ReadableStream pipeThrough(ReadableWritablePair transform, optional StreamPipeOptions options = {});
    fn pipe_through(
        &self,
        _transform: ReadableWritablePair<'js>,
        _options: Option<StreamPipeOptions<'js>>,
    ) -> Class<'js, ReadableStream<'js>> {
        unimplemented!()
    }

    // Promise<undefined> pipeTo(WritableStream destination, optional StreamPipeOptions options = {});
    async fn pipe_to(
        &self,
        _destination: Class<'js, WriteableStream>,
        _options: Option<StreamPipeOptions<'js>>,
    ) -> Result<()> {
        unimplemented!()
    }

    // sequence<ReadableStream> tee();
    fn tee(&self) -> List<(Class<'js, Self>, Class<'js, Self>)> {
        unimplemented!()
    }
}

impl<'js> ReadableStream<'js> {
    fn readable_stream_error(
        &mut self,
        ctx: &Ctx<'js>,
        reader: Option<&mut ReadableStreamReaderOwnedBorrowMut<'js>>,
        e: Value<'js>,
    ) -> Result<()> {
        // Set stream.[[state]] to "errored".
        self.state = ReadableStreamState::Errored;
        // Set stream.[[storedError]] to e.
        self.stored_error = Some(e.clone());
        // Let reader be stream.[[reader]].
        let reader = match reader {
            // If reader is undefined, return.
            None => return Ok(()),
            Some(reader) => reader,
        };

        match reader {
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(ref mut r) => {
                // Reject reader.[[closedPromise]] with e.
                r.generic
                    .reject_closed_promise
                    .as_ref()
                    .expect("ReadableStreamError called without rejection function")
                    .call((e.clone(),))?;

                // Set reader.[[closedPromise]].[[PromiseIsHandled]] to true.
                set_promise_is_handled_to_true(ctx.clone(), &r.generic.closed_promise)?;

                // If reader implements ReadableStreamDefaultReader,
                // Perform ! ReadableStreamDefaultReaderErrorReadRequests(reader, e).
                r.readable_stream_default_reader_error_read_requests(e)
            },
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(ref mut r) => {
                // Reject reader.[[closedPromise]] with e.
                r.generic
                    .reject_closed_promise
                    .as_ref()
                    .expect("ReadableStreamError called without rejection function")
                    .call((e.clone(),))?;

                // Otherwise,
                // Perform ! ReadableStreamBYOBReaderErrorReadIntoRequests(reader, e).
                r.readable_stream_byob_reader_error_read_into_requests(e)
            },
        }
    }

    fn readable_stream_has_default_reader(&self) -> bool {
        // Let reader be stream.[[reader]].
        match self.reader {
            // If reader is undefined, return false.
            None => false,
            // If reader implements ReadableStreamDefaultReader, return true.
            Some(ReadableStreamReader::ReadableStreamDefaultReader { .. }) => true,
            // Return false
            Some(ReadableStreamReader::ReadableStreamBYOBReader { .. }) => false,
        }
    }

    fn readable_stream_get_num_read_requests(
        reader: Option<&ReadableStreamReaderOwnedBorrowMut<'js>>,
    ) -> usize {
        match reader {
            Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(r)) => r.read_requests.len(),
            _ => panic!("ReadableStreamGetNumReadRequests called without checking ReadableStreamHasDefaultReader")
        }
    }

    fn readable_stream_has_byob_reader(&self) -> bool {
        // Let reader be stream.[[reader]].
        match self.reader {
            // If reader is undefined, return false.
            None => false,
            // If reader implements ReadableStreamBYOBReader, return true.
            Some(ReadableStreamReader::ReadableStreamBYOBReader { .. }) => true,
            // Return false
            Some(ReadableStreamReader::ReadableStreamDefaultReader { .. }) => false,
        }
    }

    fn readable_stream_get_num_read_into_requests(
        reader: Option<&ReadableStreamReaderOwnedBorrowMut<'js>>,
    ) -> usize {
        match reader {
            Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(r)) => r.read_into_requests.len(),
            _ => panic!("readable_stream_get_num_read_requests called without checking readable_stream_has_byob_reader")
        }
    }

    fn readable_stream_fulfill_read_request(
        &mut self,
        // Let reader be stream.[[reader]].
        reader: Option<&mut ReadableStreamReaderOwnedBorrowMut<'js>>,
        chunk: Value<'js>,
        done: bool,
    ) -> Result<()> {
        // Assert: ! ReadableStreamHasDefaultReader(stream) is true.
        let read_requests = match reader {
            Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(ref mut  r)) => &mut r.read_requests,
            _ => panic!("ReadableStreamFulfillReadRequest called on stream that doesn't satisfy ReadableStreamHasDefaultReader")
        };

        // Let readRequest be reader.[[readRequests]][0].
        // Remove readRequest from reader.[[readRequests]].
        let read_request = read_requests
            .pop_front()
            .expect("ReadableStreamFulfillReadRequest called with empty readRequests");

        if done {
            // If done is true, perform readRequest’s close steps.
            read_request.close_steps.call(())?;
        } else {
            // Otherwise, perform readRequest’s chunk steps, given chunk.
            read_request.chunk_steps.call((chunk,))?;
        }

        Ok(())
    }

    fn readable_stream_fulfill_read_into_request(
        &mut self,
        reader: Option<&mut ReadableStreamReaderOwnedBorrowMut<'js>>,
        chunk: ObjectBytes<'js>,
        done: bool,
    ) -> Result<()> {
        // Assert: ! ReadableStreamHasBYOBReader(stream) is true.
        // Let reader be stream.[[reader]].
        let read_into_requests = match reader {
            Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(r)) => &mut r.read_into_requests,
            _ => panic!("ReadableStreamFulfillReadIntoRequest called on stream that doesn't satisfy ReadableStreamHasDefaultReader")
        };

        // Let readIntoRequest be reader.[[readIntoRequests]][0].
        // Remove readIntoRequest from reader.[[readIntoRequests]].
        let read_into_request = read_into_requests
            .pop_front()
            .expect("ReadableStreamFulfillReadIntoRequest called with empty readIntoRequests");

        if done {
            // If done is true, perform readIntoRequest’s close steps, given chunk.
            read_into_request.close_steps.call((chunk,))?;
        } else {
            // Otherwise, perform readIntoRequest’s chunk steps, given chunk.
            read_into_request.chunk_steps.call((chunk,))?;
        }

        Ok(())
    }

    fn readable_stream_close(
        &mut self,
        // Let reader be stream.[[reader]].
        reader: Option<&ReadableStreamReaderOwnedBorrowMut>,
    ) -> Result<()> {
        // Set stream.[[state]] to "closed".
        self.state = ReadableStreamState::Closed;
        let reader = match reader {
            // If reader is undefined, return.
            None => return Ok(()),
            Some(reader) => reader,
        };
        match reader {
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(r) => {
                // Resolve reader.[[closedPromise]] with undefined.
                r.generic
                    .resolve_closed_promise
                    .as_ref()
                    .expect("ReadableStreamClose called without resolution function")
                    .call((Undefined,))?;

                // If reader implements ReadableStreamDefaultReader,
                // Let readRequests be reader.[[readRequests]].
                // For each readRequest of readRequests,
                for read_request in &r.read_requests {
                    // Perform readRequest’s close steps.
                    read_request.close_steps.call(())?;
                }
            },
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(r) => {
                r.generic
                    .resolve_closed_promise
                    .as_ref()
                    .expect("ReadableStreamClose called without resolution function")
                    .call((Undefined,))?;
            },
        }

        Ok(())
    }

    fn is_readable_stream_locked(&self) -> bool {
        // If stream.[[reader]] is undefined, return false.
        if self.reader.is_none() {
            return false;
        }
        // Return true.
        true
    }

    fn readable_stream_add_read_request(
        &mut self,
        reader: &mut ReadableStreamReaderOwnedBorrowMut<'js>,
        read_request: ReadableStreamReadRequest<'js>,
    ) {
        match reader {
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(_) => {
                panic!("ReadableStreamAddReadRequest called with byob reader")
            },
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(ref mut r) => {
                r.read_requests.push_back(read_request)
            },
        }
    }

    fn readable_stream_cancel(
        ctx: &Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        // Set stream.[[disturbed]] to true.
        stream.disturbed = true;

        match stream.state {
            // If stream.[[state]] is "closed", return a promise resolved with undefined.
            ReadableStreamState::Closed => {
                return promise_resolved_with(ctx, Ok(Value::new_undefined(ctx.clone())));
            },
            // If stream.[[state]] is "errored", return a promise rejected with stream.[[storedError]].
            ReadableStreamState::Errored => {
                return promise_rejected_with(
                    ctx,
                    stream
                        .stored_error
                        .clone()
                        .expect("ReadableStream in errored state without a stored error"),
                );
            },
            ReadableStreamState::Readable => {
                // Perform ! ReadableStreamClose(stream).
                stream.readable_stream_close(reader.as_ref())?;
                // Let reader be stream.[[reader]].
                // If reader is not undefined and reader implements ReadableStreamBYOBReader,
                if let Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(
                    ref mut r,
                )) = reader
                {
                    // Let readIntoRequests be reader.[[readIntoRequests]].
                    // Set reader.[[readIntoRequests]] to an empty list.
                    let read_into_requests = r.read_into_requests.split_off(0);
                    // For each readIntoRequest of readIntoRequests,
                    for read_into_request in read_into_requests {
                        // Perform readIntoRequest’s close steps, given undefined.
                        read_into_request.close_steps.call((Undefined,))?
                    }
                }

                let mut controller = stream
                    .controller
                    .clone()
                    .expect("ReadableStreamCancel called without a controller");

                // Let sourceCancelPromise be ! stream.[[controller]].[[CancelSteps]](reason).
                let source_cancel_promise = controller.cancel_steps(ctx, stream, reader, reason)?;

                // Return the result of reacting to sourceCancelPromise with a fulfillment step that returns undefined.
                source_cancel_promise.then()?.call((
                    This(source_cancel_promise.clone()),
                    Function::new(ctx.clone(), || Undefined)?,
                ))
            },
        }
    }

    fn readable_stream_add_read_into_request(
        &mut self,
        read_request: ReadableStreamReadIntoRequest<'js>,
    ) {
        // Assert: stream.[[reader]] implements ReadableStreamBYOBReader.
        match self.reader_mut() {
            Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(mut r)) => {
                // Append readRequest to stream.[[reader]].[[readIntoRequests]].
                r.read_into_requests.push_back(read_request)
            },
            _ => {
                panic!("ReadableStreamAddReadIntoRequest called on stream without ReadableStreamBYOBReader");
            },
        }
    }

    fn reader_mut(&mut self) -> Option<ReadableStreamReaderOwnedBorrowMut<'js>> {
        self.reader.as_mut().map(|r| r.borrow_mut())
    }
}

struct UnderlyingSource<'js> {
    // callback UnderlyingSourceStartCallback = any (ReadableStreamController controller);
    start: Option<Function<'js>>,
    // callback UnderlyingSourcePullCallback = Promise<undefined> (ReadableStreamController controller);
    pull: Option<Function<'js>>,
    // callback UnderlyingSourceCancelCallback = Promise<undefined> (optional any reason);
    cancel: Option<Function<'js>>,
    r#type: Option<ReadableStreamType>,
    // [EnforceRange] unsigned long long autoAllocateChunkSize;
    auto_allocate_chunk_size: Option<usize>,
}

impl<'js> FromJs<'js> for UnderlyingSource<'js> {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let start = obj.get_optional::<_, _>("start")?;
        let pull = obj.get_optional::<_, _>("pull")?;
        let cancel = obj.get_optional::<_, _>("cancel")?;
        let r#type = obj.get_optional::<_, _>("type")?;
        let auto_allocate_chunk_size = obj.get_optional::<_, _>("autoAllocateChunkSize")?;

        Ok(Self {
            start,
            pull,
            cancel,
            r#type,
            auto_allocate_chunk_size,
        })
    }
}

// enum ReadableStreamType { "bytes" };
enum ReadableStreamType {
    Bytes,
}

impl<'js> FromJs<'js> for ReadableStreamType {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let str = value
            .as_string()
            .ok_or(Error::new_from_js(ty_name, "String"))?;

        match str.to_string()?.as_str() {
            "bytes" => Ok(Self::Bytes),
            _ => Err(Error::new_from_js(ty_name, "ReadableStreamType")),
        }
    }
}

struct QueuingStrategy<'js> {
    // unrestricted double highWaterMark;
    high_water_mark: Option<Value<'js>>,
    // callback QueuingStrategySize = unrestricted double (any chunk);
    size: Option<Function<'js>>,
}

impl<'js> FromJs<'js> for QueuingStrategy<'js> {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let high_water_mark = obj.get_optional::<_, Value>("highWaterMark")?;
        let size = obj.get_optional::<_, _>("size")?;

        Ok(Self {
            high_water_mark,
            size,
        })
    }
}

impl<'js> QueuingStrategy<'js> {
    // https://streams.spec.whatwg.org/#validate-and-normalize-high-water-mark
    fn extract_high_water_mark(
        ctx: &Ctx<'js>,
        this: &Option<QueuingStrategy<'js>>,
        default_hwm: f64,
    ) -> Result<f64> {
        match this {
            // If strategy["highWaterMark"] does not exist, return defaultHWM.
            None => Ok(default_hwm),
            Some(this) => {
                // Let highWaterMark be strategy["highWaterMark"].
                if let Some(high_water_mark) = &this.high_water_mark {
                    let high_water_mark = high_water_mark.as_number().unwrap_or(f64::NAN);
                    // If highWaterMark is NaN or highWaterMark < 0, throw a RangeError exception.
                    if high_water_mark.is_nan() || high_water_mark < 0.0 {
                        Err(Exception::throw_range(ctx, "Invalid highWaterMark"))
                    } else {
                        // Return highWaterMark.
                        Ok(high_water_mark)
                    }
                } else {
                    // If strategy["highWaterMark"] does not exist, return defaultHWM.
                    Ok(default_hwm)
                }
            },
        }
    }

    // https://streams.spec.whatwg.org/#make-size-algorithm-from-size-function
    fn extract_size_algorithm(this: &Option<QueuingStrategy<'js>>) -> SizeAlgorithm<'js> {
        // If strategy["size"] does not exist, return an algorithm that returns 1.
        match this.as_ref().and_then(|t| t.size.as_ref()) {
            None => SizeAlgorithm::AlwaysOne,
            Some(size) => SizeAlgorithm::SizeFunction(size.clone()),
        }
    }
}

struct ReadableStreamGetReaderOptions {
    mode: Option<ReadableStreamReaderMode>,
}

impl<'js> FromJs<'js> for ReadableStreamGetReaderOptions {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let mode = obj.get_optional::<_, ReadableStreamReaderMode>("mode")?;

        Ok(Self { mode })
    }
}

// enum ReadableStreamReaderMode { "byob" };
enum ReadableStreamReaderMode {
    Byob,
}

impl<'js> FromJs<'js> for ReadableStreamReaderMode {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let typ = value.type_of();
        let mode = match typ {
            Type::String => value.into_string().unwrap(),
            Type::Object => {
                if let Some(to_string) = value.get_optional::<_, Function>("toString")? {
                    to_string.call(())?
                } else {
                    return Err(Error::new_from_js("Object", "String"));
                }
            },
            typ => return Err(Error::new_from_js(typ.as_str(), "String")),
        };

        match mode.to_string()?.as_str() {
            "byob" => Ok(Self::Byob),
            _ => Err(Error::new_from_js(typ.as_str(), "ReadableStreamReaderMode")),
        }
    }
}

pub struct ReadableStreamGenericReader<'js> {
    resolve_closed_promise: Option<Function<'js>>,
    reject_closed_promise: Option<Function<'js>>,
    closed_promise: Promise<'js>,
    stream: Option<Class<'js, ReadableStream<'js>>>,
}

impl<'js> ReadableStreamGenericReader<'js> {
    fn readable_stream_reader_generic_initialize(
        ctx: &Ctx<'js>,
        stream: OwnedBorrow<'js, ReadableStream<'js>>,
    ) -> Result<Self> {
        let (closed_promise, resolve_closed_promise, reject_closed_promise) = match stream.state {
            // If stream.[[state]] is "readable",
            ReadableStreamState::Readable => {
                // Set reader.[[closedPromise]] to a new promise.
                let (promise, resolve, reject) = Promise::new(ctx)?;
                (promise, Some(resolve), Some(reject))
            },
            // Otherwise, if stream.[[state]] is "closed",
            ReadableStreamState::Closed => {
                // Set reader.[[closedPromise]] to a promise resolved with undefined.
                (
                    promise_resolved_with(ctx, Ok(Value::new_undefined(ctx.clone())))?,
                    None,
                    None,
                )
            },
            // Otherwise,
            ReadableStreamState::Errored => {
                // Set reader.[[closedPromise]] to a promise rejected with stream.[[storedError]].
                // Set reader.[[closedPromise]].[[PromiseIsHandled]] to true.
                (
                    {
                        let promise = promise_rejected_with(
                            ctx,
                            stream
                                .stored_error
                                .clone()
                                .expect("ReadableStream in errored state without a stored error"),
                        )?;
                        set_promise_is_handled_to_true(ctx.clone(), &promise)?;
                        promise
                    },
                    None,
                    None,
                )
            },
        };
        Ok(Self {
            // Set reader.[[stream]] to stream.
            stream: Some(stream.into_inner()),
            resolve_closed_promise,
            reject_closed_promise,
            closed_promise,
        })
    }

    fn readable_stream_reader_generic_release(&mut self, ctx: &Ctx<'js>) -> Result<()> {
        // Let stream be reader.[[stream]].
        // Assert: stream is not undefined.

        let stream = self
            .stream
            .clone()
            .expect("ReadableStreamReaderGenericRelease called without stream");

        // If stream.[[state]] is "readable", reject reader.[[closedPromise]] with a TypeError exception.
        if let ReadableStreamState::Readable = stream.borrow().state {
            let e: Value = ctx.eval(r#"new TypeError("Reader was released and can no longer be used to monitor the stream's closedness")"#)?;
            self.reject_closed_promise
                .as_ref()
                .expect("ReadableStreamReaderGenericRelease called without rejection function")
                .call((e,))?;
        } else {
            // Otherwise, set reader.[[closedPromise]] to a promise rejected with a TypeError exception.
            let e: Value = ctx.eval(r#"new TypeError("Reader was released and can no longer be used to monitor the stream's closedness")"#)?;
            self.closed_promise = promise_rejected_with(ctx, e)?;
        }

        // Set reader.[[closedPromise]].[[PromiseIsHandled]] to true.
        set_promise_is_handled_to_true(ctx.clone(), &self.closed_promise)?;

        // Perform ! stream.[[controller]].[[ReleaseSteps]]().
        let controller = stream
            .borrow()
            .controller
            .clone()
            .expect("ReadableStreamReaderGenericRelease called without stream controller");
        controller.release_steps();

        // Set stream.[[reader]] to undefined.
        stream.borrow_mut().reader = None;

        // Set reader.[[stream]] to undefined.
        self.stream = None;

        Ok(())
    }

    fn readable_stream_reader_generic_cancel(
        ctx: Ctx<'js>,
        reader: ReadableStreamReaderOwnedBorrowMut<'js>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        // Let stream be reader.[[stream]].
        let stream = match reader.generic().stream {
            // Assert: stream is not undefined.
            None => {
                panic!("ReadableStreamReaderGenericCancel called without stream")
            },
            Some(ref stream) => OwnedBorrowMut::from_class(stream.clone()),
        };

        // Return ! ReadableStreamCancel(stream, reason).
        ReadableStream::readable_stream_cancel(&ctx, stream, Some(reader), reason)
    }
}

impl<'js> Trace<'js> for ReadableStreamGenericReader<'js> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        self.resolve_closed_promise.trace(tracer);
        self.reject_closed_promise.trace(tracer);
        self.closed_promise.trace(tracer);
        self.stream.trace(tracer);
    }
}

// typedef (ReadableStreamDefaultController or ReadableByteStreamController) ReadableStreamController;
#[derive(Clone)]
enum ReadableStreamReader<'js> {
    ReadableStreamDefaultReader(Class<'js, ReadableStreamDefaultReader<'js>>),
    ReadableStreamBYOBReader(Class<'js, ReadableStreamBYOBReader<'js>>),
}

impl<'js> ReadableStreamReader<'js> {
    fn borrow_mut(&mut self) -> ReadableStreamReaderOwnedBorrowMut<'js> {
        match self {
            Self::ReadableStreamDefaultReader(r) => {
                ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(
                    OwnedBorrowMut::from_class(Class::clone(r)),
                )
            },
            Self::ReadableStreamBYOBReader(r) => {
                ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(
                    OwnedBorrowMut::from_class(Class::clone(r)),
                )
            },
        }
    }
}

enum ReadableStreamReaderOwnedBorrowMut<'js> {
    ReadableStreamDefaultReader(OwnedBorrowMut<'js, ReadableStreamDefaultReader<'js>>),
    ReadableStreamBYOBReader(OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>),
}

impl<'js> ReadableStreamReaderOwnedBorrowMut<'js> {
    fn generic(&self) -> &ReadableStreamGenericReader<'js> {
        match self {
            Self::ReadableStreamDefaultReader(r) => &r.generic,
            Self::ReadableStreamBYOBReader(r) => &r.generic,
        }
    }

    fn into_inner(self) -> ReadableStreamReader<'js> {
        match self {
            Self::ReadableStreamDefaultReader(r) => {
                ReadableStreamReader::ReadableStreamDefaultReader(r.into_inner())
            },
            Self::ReadableStreamBYOBReader(r) => {
                ReadableStreamReader::ReadableStreamBYOBReader(r.into_inner())
            },
        }
    }

    fn from_class(class: ReadableStreamReader<'js>) -> Self {
        match class {
            ReadableStreamReader::ReadableStreamDefaultReader(r) => {
                Self::ReadableStreamDefaultReader(OwnedBorrowMut::from_class(r))
            },
            ReadableStreamReader::ReadableStreamBYOBReader(r) => {
                Self::ReadableStreamBYOBReader(OwnedBorrowMut::from_class(r))
            },
        }
    }
}

impl<'js> Trace<'js> for ReadableStreamReader<'js> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        match self {
            Self::ReadableStreamDefaultReader(r) => r.trace(tracer),
            Self::ReadableStreamBYOBReader(r) => r.trace(tracer),
        }
    }
}

impl<'js> FromJs<'js> for ReadableStreamReader<'js> {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        if let Ok(default) = obj.into_class() {
            return Ok(Self::ReadableStreamDefaultReader(default));
        }

        if let Ok(default) = obj.into_class() {
            return Ok(Self::ReadableStreamBYOBReader(default));
        }

        Err(Error::new_from_js(ty_name, "ReadableStreamReader"))
    }
}

impl<'js> ReadableStreamReader<'js> {
    fn acquire_readable_stream_default_reader(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<()> {
        ReadableStreamDefaultReader::new(ctx, stream)?;
        Ok(())
    }

    fn acquire_readable_stream_byob_reader(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<()> {
        ReadableStreamBYOBReader::new(ctx, stream)?;
        Ok(())
    }
}

impl<'js> IntoJs<'js> for ReadableStreamReader<'js> {
    fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
        match self {
            Self::ReadableStreamDefaultReader(r) => r.into_js(ctx),
            Self::ReadableStreamBYOBReader(r) => r.into_js(ctx),
        }
    }
}

struct StreamPipeOptions<'js> {
    prevent_close: bool,
    prevent_abort: bool,
    prevent_cancel: bool,
    signal: Option<AbortSignal<'js>>,
}

impl<'js> FromJs<'js> for StreamPipeOptions<'js> {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let prevent_close = obj
            .get_optional::<_, bool>("prevent_close")?
            .unwrap_or(false);
        let prevent_abort = obj
            .get_optional::<_, bool>("prevent_abort")?
            .unwrap_or(false);
        let prevent_cancel = obj
            .get_optional::<_, bool>("prevent_cancel")?
            .unwrap_or(false);

        let signal = obj.get_optional::<_, AbortSignal<'js>>("signal")?;

        Ok(Self {
            prevent_close,
            prevent_abort,
            prevent_cancel,
            signal,
        })
    }
}

#[derive(Trace, Clone)]
enum ReadableStreamController<'js> {
    ReadableStreamDefaultController(Class<'js, ReadableStreamDefaultController<'js>>),
    ReadableStreamByteController(Class<'js, ReadableStreamByteController<'js>>),
}

impl<'js> IntoJs<'js> for ReadableStreamController<'js> {
    fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
        match self {
            Self::ReadableStreamDefaultController(c) => c.into_js(ctx),
            Self::ReadableStreamByteController(c) => c.into_js(ctx),
        }
    }
}

impl<'js> ReadableStreamController<'js> {
    fn pull_steps(
        &self,
        ctx: &Ctx<'js>,
        reader: ReadableStreamReaderOwnedBorrowMut<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        match self {
            Self::ReadableStreamDefaultController(c) => {
                let controller = OwnedBorrowMut::from_class(c.clone());
                ReadableStreamDefaultController::pull_steps(
                    ctx,
                    controller,
                    reader,
                    stream,
                    read_request,
                )
            },
            Self::ReadableStreamByteController(c) => {
                let controller = OwnedBorrowMut::from_class(c.clone());
                ReadableStreamByteController::pull_steps(
                    ctx,
                    controller,
                    reader,
                    stream,
                    read_request,
                )
            },
        }
    }

    fn cancel_steps(
        &mut self,
        ctx: &Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        match self {
            Self::ReadableStreamDefaultController(c) => {
                let controller = OwnedBorrowMut::from_class(c.clone());
                ReadableStreamDefaultController::cancel_steps(
                    ctx, controller, stream, reader, reason,
                )
            },
            Self::ReadableStreamByteController(c) => {
                let controller = OwnedBorrowMut::from_class(c.clone());
                ReadableStreamByteController::cancel_steps(ctx, controller, stream, reader, reason)
            },
        }
    }

    fn release_steps(&self) {
        match self {
            Self::ReadableStreamDefaultController(c) => c.borrow_mut().release_steps(),
            Self::ReadableStreamByteController(c) => c.borrow_mut().release_steps(),
        }
    }
}

fn transfer_array_buffer<'js>(
    ctx: Ctx<'js>,
    mut buffer: ArrayBuffer<'js>,
) -> Result<ArrayBuffer<'js>> {
    let bytes: &[u8] = buffer
        .as_slice()
        .expect("TransferArrayBuffer called on detached buffer");
    let copied_array_buffer = ArrayBuffer::new_copy(ctx, bytes)?;

    // Perform ? DetachArrayBuffer(O).
    buffer.detach();

    // Return a new ArrayBuffer object, created in the current Realm, whose [[ArrayBufferData]] internal slot value is arrayBufferData and whose [[ArrayBufferByteLength]] internal slot value is arrayBufferByteLength.
    // zero copy is not possible as quickjs does not expose the transfer functionality yet
    Ok(copied_array_buffer)
}

fn copy_data_block_bytes(
    ctx: &Ctx<'_>,
    to_block: ArrayBuffer,
    to_index: usize,
    from_block: ArrayBuffer,
    from_index: usize,
    count: usize,
) -> Result<()> {
    let to_raw = to_block
        .as_raw()
        .ok_or(ERROR_MSG_ARRAY_BUFFER_DETACHED)
        .or_throw(ctx)?;
    let to_slice = unsafe { std::slice::from_raw_parts_mut(to_raw.ptr.as_ptr(), to_raw.len) };
    let from_raw = from_block
        .as_raw()
        .ok_or(ERROR_MSG_ARRAY_BUFFER_DETACHED)
        .or_throw(ctx)?;
    let from_slice = unsafe { std::slice::from_raw_parts(from_raw.ptr.as_ptr(), from_raw.len) };

    to_slice[to_index..to_index + count]
        .copy_from_slice(&from_slice[from_index..from_index + count]);
    Ok(())
}

fn promise_resolved_with<'js>(ctx: &Ctx<'js>, value: Result<Value<'js>>) -> Result<Promise<'js>> {
    let (promise, resolve, reject) = Promise::new(ctx)?;
    match value {
        Ok(value) => resolve.call((value,))?,
        Err(Error::Exception) => reject.call((ctx.catch(),))?,
        Err(err) => return Err(err),
    }

    Ok(promise)
}

fn promise_rejected_with<'js>(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Promise<'js>> {
    let (promise, _, reject) = Promise::new(ctx)?;
    reject.call((value,))?;

    Ok(promise)
}

// https://webidl.spec.whatwg.org/#dfn-perform-steps-once-promise-is-settled
fn upon_promise<'js, Input: FromJs<'js> + 'js, Output: IntoJs<'js> + 'js>(
    ctx: Ctx<'js>,
    promise: Promise<'js>,
    then: impl FnOnce(std::result::Result<Input, Value<'js>>) -> Result<Output> + 'js,
) -> Result<Promise<'js>> {
    let then = Rc::new(Cell::new(Some(then)));
    let then2 = then.clone();
    promise.then()?.call((
        This(promise.clone()),
        Function::new(
            ctx.clone(),
            OnceFn::new(move |input| {
                then.take()
                    .expect("Promise.then should only call either resolve or reject")(
                    Ok(input)
                )
            }),
        ),
        Function::new(
            ctx,
            OnceFn::new(move |e: Value<'js>| {
                then2
                    .take()
                    .expect("Promise.then should only call either resolve or reject")(
                    Err(e)
                )
            }),
        ),
    ))
}

fn set_promise_is_handled_to_true<'js>(ctx: Ctx<'js>, promise: &Promise<'js>) -> Result<()> {
    promise
        .then()?
        .call((This(promise.clone()), Undefined, Function::new(ctx, || {})))
}

#[derive(Trace, Clone)]
enum SizeAlgorithm<'js> {
    AlwaysOne,
    SizeFunction(Function<'js>),
}

impl<'js> SizeAlgorithm<'js> {
    fn call(&self, ctx: Ctx<'js>, chunk: Value<'js>) -> Result<Value<'js>> {
        match self {
            Self::AlwaysOne => Ok(Value::new_number(ctx, 1.0)),
            Self::SizeFunction(ref f) => f.call((chunk.clone(),)),
        }
    }
}

enum StartAlgorithm<'js> {
    ReturnUndefined,
    Function {
        f: Function<'js>,
        underlying_source: Value<'js>,
    },
}

impl<'js> StartAlgorithm<'js> {
    fn call(&self, ctx: Ctx<'js>, controller: ReadableStreamController<'js>) -> Result<Value<'js>> {
        match self {
            StartAlgorithm::ReturnUndefined => Ok(Value::new_undefined(ctx.clone())),
            StartAlgorithm::Function {
                f,
                underlying_source,
            } => f.call::<_, Value>((This(underlying_source.clone()), controller)),
        }
    }
}

#[derive(Trace, Clone)]
enum PullAlgorithm<'js> {
    ReturnPromiseUndefined,
    Function {
        f: Function<'js>,
        underlying_source: Value<'js>,
    },
}

impl<'js> PullAlgorithm<'js> {
    fn call(
        &self,
        ctx: Ctx<'js>,
        controller: ReadableStreamController<'js>,
    ) -> Result<Promise<'js>> {
        match self {
            PullAlgorithm::ReturnPromiseUndefined => Ok(promise_resolved_with(
                &ctx,
                Ok(Value::new_undefined(ctx.clone())),
            )?),
            PullAlgorithm::Function {
                f,
                underlying_source,
            } => promise_resolved_with(
                &ctx,
                f.call::<_, Value>((This(underlying_source.clone()), controller)),
            ),
        }
    }
}

#[derive(Trace, Clone)]
enum CancelAlgorithm<'js> {
    ReturnPromiseUndefined,
    Function {
        f: Function<'js>,
        underlying_source: Value<'js>,
    },
}

impl<'js> CancelAlgorithm<'js> {
    fn call(&self, ctx: Ctx<'js>, reason: Value<'js>) -> Result<Promise<'js>> {
        match self {
            CancelAlgorithm::ReturnPromiseUndefined => Ok(promise_resolved_with(
                &ctx,
                Ok(Value::new_undefined(ctx.clone())),
            )?),
            CancelAlgorithm::Function {
                f,
                underlying_source,
            } => promise_resolved_with(
                &ctx,
                f.call::<_, Value>((This(underlying_source.clone()), reason)),
            ),
        }
    }
}

#[derive(Trace, Clone)]
struct ReadableStreamReadRequest<'js> {
    chunk_steps: Function<'js>,
    close_steps: Function<'js>,
    error_steps: Function<'js>,
}

struct ReadableStreamReadResult<'js> {
    value: Value<'js>,
    done: bool,
}

impl<'js> IntoJs<'js> for ReadableStreamReadResult<'js> {
    fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
        let obj = Object::new(ctx.clone())?;
        obj.set("value", self.value)?;
        obj.set("done", self.done)?;
        Ok(obj.into_value())
    }
}

fn downgrade_owned_borrow_mut<'js, T: JsClass<'js>>(
    borrow: OwnedBorrowMut<'js, T>,
) -> OwnedBorrow<'js, T> {
    OwnedBorrow::from_class(borrow.into_inner())
}

fn class_from_owned_borrow_mut<'js, T: JsClass<'js>>(
    borrow: OwnedBorrowMut<'js, T>,
) -> (Class<'js, T>, OwnedBorrowMut<'js, T>) {
    let class = borrow.into_inner();
    let borrow = OwnedBorrowMut::from_class(class.clone());
    (class, borrow)
}
