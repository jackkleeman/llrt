use crate::modules::events::abort_signal::AbortSignal;
use byob_reader::ReadableStreamBYOBReader;
use byte_controller::ReadableStreamByteController;
use default_controller::ReadableStreamDefaultController;
use default_reader::ReadableStreamDefaultReader;
use llrt_utils::{
    bytes::ObjectBytes, error_messages::ERROR_MSG_ARRAY_BUFFER_DETACHED, object::ObjectExt,
    result::ResultExt,
};
use rquickjs::{
    class::{Trace, Tracer},
    prelude::{List, This},
    ArrayBuffer, Class, Ctx, Error, Exception, FromJs, Function, IntoJs, Object, Promise, Result,
    Undefined, Value,
};

use super::{writeable::WriteableStream, ReadableWritablePair};

mod byob_reader;
mod byte_controller;
mod default_controller;
mod default_reader;

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

#[rquickjs::methods]
impl<'js> ReadableStream<'js> {
    // Streams Spec: 4.2.4: https://streams.spec.whatwg.org/#rs-prototype
    // constructor(optional object underlyingSource, optional QueuingStrategy strategy = {});
    #[qjs(constructor)]
    fn new(
        ctx: Ctx<'js>,
        underlying_source: Option<UnderlyingSource<'js>>,
        queuing_strategy: Option<QueuingStrategy<'js>>,
    ) -> Result<Class<'js, Self>> {
        let this = Class::instance(
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

        match underlying_source.as_ref().and_then(|s| s.r#type.as_ref()) {
            // If underlyingSourceDict["type"] is "bytes":
            Some(ReadableStreamType::Bytes) => {
                // If strategy["size"] exists, throw a RangeError exception.
                if queuing_strategy
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
                    this.clone(),
                    underlying_source,
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
                    this.clone(),
                    underlying_source,
                    high_water_mark,
                    size_algorithm,
                )?;
            },
        }

        Ok(this)
    }

    // static ReadableStream from(any asyncIterable);
    #[qjs(static)]
    fn from(_async_iterable: Value<'js>) -> Class<'js, Self> {
        unimplemented!()
    }

    // readonly attribute boolean locked;
    #[qjs(get)]
    fn locked(stream: This<Class<'js, Self>>) -> bool {
        // Return ! IsReadableStreamLocked(this).
        stream.0.borrow().is_readable_stream_locked()
    }

    // Promise<undefined> cancel(optional any reason);
    fn cancel(
        stream: This<Class<'js, Self>>,
        ctx: Ctx<'js>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        // If ! IsReadableStreamLocked(this) is true, return a promise rejected with a TypeError exception.
        if stream.0.borrow().is_readable_stream_locked() {
            let e: Value =
                ctx.eval(r#"new TypeError("Cannot cancel a stream that already has a reader")"#)?;
            return promise_rejected_with(&ctx, e);
        }
        stream.0.borrow_mut().readable_stream_cancel(&ctx, reason)
    }

    // ReadableStreamReader getReader(optional ReadableStreamGetReaderOptions options = {});
    fn get_reader(
        _stream: This<Class<'js, Self>>,
        _options: Option<ReadableStreamGetReaderOptions>,
    ) -> ReadableStreamReader {
        unimplemented!()
    }

    // ReadableStream pipeThrough(ReadableWritablePair transform, optional StreamPipeOptions options = {});
    fn pipe_through(
        _stream: This<Class<'js, Self>>,
        _transform: ReadableWritablePair<'js>,
        _options: Option<StreamPipeOptions<'js>>,
    ) -> Class<'js, ReadableStream<'js>> {
        unimplemented!()
    }

    // Promise<undefined> pipeTo(WritableStream destination, optional StreamPipeOptions options = {});
    async fn pipe_to(
        _stream: This<Class<'js, Self>>,
        _destination: Class<'js, WriteableStream>,
        _options: Option<StreamPipeOptions<'js>>,
    ) -> Result<()> {
        unimplemented!()
    }

    // sequence<ReadableStream> tee();
    fn tee(_stream: This<Class<'js, Self>>) -> List<(Class<'js, Self>, Class<'js, Self>)> {
        unimplemented!()
    }
}

impl<'js> ReadableStream<'js> {
    fn readable_stream_error(&mut self, e: Value<'js>) -> Result<()> {
        // Set stream.[[state]] to "errored".
        self.state = ReadableStreamState::Errored;
        // Set stream.[[storedError]] to e.
        self.stored_error = Some(e.clone());
        // Let reader be stream.[[reader]].
        let reader = match self.reader {
            // If reader is undefined, return.
            None => return Ok(()),
            Some(ref reader) => reader,
        };

        match reader {
            ReadableStreamReader::ReadableStreamDefaultReader(r) => {
                // Reject reader.[[closedPromise]] with e.
                r.borrow()
                    .generic
                    .reject_closed_promise
                    .as_ref()
                    .expect("ReadableStreamError called without rejection function")
                    .call((e.clone(),))?;

                // If reader implements ReadableStreamDefaultReader,
                // Perform ! ReadableStreamDefaultReaderErrorReadRequests(reader, e).
                ReadableStreamDefaultReader::readable_stream_default_reader_error_read_requests(
                    r.clone(),
                    e,
                )
            },
            ReadableStreamReader::ReadableStreamBYOBReader(r) => {
                // Reject reader.[[closedPromise]] with e.
                r.borrow()
                    .generic
                    .reject_closed_promise
                    .as_ref()
                    .expect("ReadableStreamError called without rejection function")
                    .call((e.clone(),))?;

                // Otherwise,
                // Perform ! ReadableStreamBYOBReaderErrorReadIntoRequests(reader, e).
                ReadableStreamBYOBReader::readable_stream_byob_reader_error_read_into_requests(
                    r.clone(),
                    e,
                )
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

    fn readable_stream_get_num_read_requests(&self) -> usize {
        match self.reader {
            Some(ReadableStreamReader::ReadableStreamDefaultReader(ref r)) => r.borrow().read_requests.len(),
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

    fn readable_stream_get_num_read_into_requests(&self) -> usize {
        match self.reader {
            Some(ReadableStreamReader::ReadableStreamBYOBReader(ref r)) => r.borrow().read_into_requests.len(),
            _ => panic!("readable_stream_get_num_read_requests called without checking readable_stream_has_byob_reader")
        }
    }

    fn readable_stream_fulfill_read_request(&self, chunk: Value<'js>, done: bool) -> Result<()> {
        // Assert: ! ReadableStreamHasDefaultReader(stream) is true.
        // Let reader be stream.[[reader]].
        let read_requests = match self.reader {
            Some(ReadableStreamReader::ReadableStreamDefaultReader(ref r)) => &mut r.borrow_mut().read_requests,
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
        stream: Class<'js, Self>,
        chunk: ObjectBytes<'js>,
        done: bool,
    ) -> Result<()> {
        let stream = stream.borrow();
        // Assert: ! ReadableStreamHasBYOBReader(stream) is true.
        // Let reader be stream.[[reader]].
        let read_into_requests = match stream.reader {
            Some(ReadableStreamReader::ReadableStreamBYOBReader(ref r)) => &mut r.borrow_mut().read_into_requests,
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

    fn readable_stream_close(&mut self) -> Result<()> {
        // Set stream.[[state]] to "closed".
        self.state = ReadableStreamState::Closed;
        // Let reader be stream.[[reader]].
        let reader = match &self.reader {
            // If reader is undefined, return.
            None => return Ok(()),
            Some(reader) => reader,
        };
        match reader {
            ReadableStreamReader::ReadableStreamDefaultReader(r) => {
                let reader = r.borrow();
                // Resolve reader.[[closedPromise]] with undefined.
                reader
                    .generic
                    .resolve_closed_promise
                    .as_ref()
                    .expect("ReadableStreamClose called without resolution function")
                    .call((Undefined,))?;

                // If reader implements ReadableStreamDefaultReader,
                // Let readRequests be reader.[[readRequests]].
                // For each readRequest of readRequests,
                for read_request in &reader.read_requests {
                    // Perform readRequest’s close steps.
                    read_request.close_steps.call(())?;
                }
            },
            ReadableStreamReader::ReadableStreamBYOBReader(r) => {
                r.borrow()
                    .generic
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

    fn readable_stream_add_read_request(&self, read_request: ReadableStreamReadRequest<'js>) {
        match self.reader {
            None => panic!("ReadableStreamAddReadRequest called without reader"),
            Some(ReadableStreamReader::ReadableStreamBYOBReader(_)) => {
                panic!("ReadableStreamAddReadRequest called with byob reader")
            },
            Some(ReadableStreamReader::ReadableStreamDefaultReader(ref r)) => {
                r.borrow_mut().read_requests.push_back(read_request)
            },
        }
    }

    fn readable_stream_cancel(
        &mut self,
        ctx: &Ctx<'js>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        // Set stream.[[disturbed]] to true.
        self.disturbed = true;

        match self.state {
            // If stream.[[state]] is "closed", return a promise resolved with undefined.
            ReadableStreamState::Closed => {
                return promise_resolved_with(ctx, Ok(Value::new_undefined(ctx.clone())));
            },
            // If stream.[[state]] is "errored", return a promise rejected with stream.[[storedError]].
            ReadableStreamState::Errored => {
                return promise_rejected_with(
                    ctx,
                    self.stored_error
                        .clone()
                        .expect("ReadableStream in errored state without a stored error"),
                );
            },
            ReadableStreamState::Readable => {
                // Perform ! ReadableStreamClose(stream).
                self.readable_stream_close()?;
                // Let reader be stream.[[reader]].
                let reader = &self.reader;
                if let Some(ReadableStreamReader::ReadableStreamBYOBReader(r)) = reader {
                    // Let readIntoRequests be reader.[[readIntoRequests]].
                    // Set reader.[[readIntoRequests]] to an empty list.
                    let read_into_requests = r.borrow_mut().read_into_requests.split_off(0);
                    // For each readIntoRequest of readIntoRequests,
                    for read_into_request in read_into_requests {
                        // Perform readIntoRequest’s close steps, given undefined.
                        read_into_request.close_steps.call((Undefined,))?
                    }
                }

                let controller = self
                    .controller
                    .clone()
                    .expect("ReadableStreamCancel called without a controller");

                // Let sourceCancelPromise be ! stream.[[controller]].[[CancelSteps]](reason).
                let source_cancel_promise = controller.cancel_steps(ctx, reason)?;

                // Return the result of reacting to sourceCancelPromise with a fulfillment step that returns undefined.
                source_cancel_promise
                    .then()?
                    .call((Function::new(ctx.clone(), || Undefined)?,))
            },
        }
    }
}

struct UnderlyingSource<'js> {
    js: Object<'js>,
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
            js: obj.clone(),
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
    high_water_mark: Option<f64>,
    // callback QueuingStrategySize = unrestricted double (any chunk);
    size: Option<Function<'js>>,
}

impl<'js> FromJs<'js> for QueuingStrategy<'js> {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let high_water_mark = obj.get_optional::<_, _>("highWaterMark")?;
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
                if let Some(high_water_mark) = this.high_water_mark {
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

#[derive(Trace)]
enum SizeAlgorithm<'js> {
    AlwaysOne,
    SizeFunction(Function<'js>),
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
        let ty_name = value.type_name();
        let str = value
            .as_string()
            .ok_or(Error::new_from_js(ty_name, "String"))?;

        match str.to_string()?.as_str() {
            "byob" => Ok(Self::Byob),
            _ => Err(Error::new_from_js(ty_name, "ReadableStreamReaderMode")),
        }
    }
}

pub struct ReadableStreamGenericReader<'js> {
    resolve_closed_promise: Option<Function<'js>>,
    reject_closed_promise: Option<Function<'js>>,
    closed_promise: Promise<'js>,
    stream: Class<'js, ReadableStream<'js>>,
}

impl<'js> ReadableStreamGenericReader<'js> {
    fn readable_stream_generic_initialize(
        ctx: &Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
    ) -> Result<Self> {
        let (closed_promise, resolve_closed_promise, reject_closed_promise) =
            match stream.borrow().state {
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
                        promise_rejected_with(
                            ctx,
                            stream
                                .borrow()
                                .stored_error
                                .clone()
                                .expect("ReadableStream in errored state without a stored error"),
                        )?,
                        None,
                        None,
                    )
                },
            };
        Ok(Self {
            // Set reader.[[stream]] to stream.
            stream,
            resolve_closed_promise,
            reject_closed_promise,
            closed_promise,
        })
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
#[derive(Trace)]
enum ReadableStreamReader<'js> {
    ReadableStreamDefaultReader(Class<'js, ReadableStreamDefaultReader<'js>>),
    ReadableStreamBYOBReader(Class<'js, ReadableStreamBYOBReader<'js>>),
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

impl<'js> ReadableStreamController<'js> {
    fn pull_steps(
        &self,
        ctx: &Ctx<'js>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        match self {
            Self::ReadableStreamDefaultController(c) => {
                ReadableStreamDefaultController::pull_steps(c.clone(), ctx, read_request)
            },
            Self::ReadableStreamByteController(c) => {
                ReadableStreamByteController::pull_steps(c.clone(), ctx, read_request)
            },
        }
    }

    fn cancel_steps(&self, ctx: &Ctx<'js>, reason: Value<'js>) -> Result<Promise<'js>> {
        match self {
            Self::ReadableStreamDefaultController(c) => {
                ReadableStreamDefaultController::cancel_steps(c.clone(), ctx, reason)
            },
            Self::ReadableStreamByteController(c) => {
                ReadableStreamByteController::cancel_steps(c.clone(), ctx, reason)
            },
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
fn upon_promise<'js>(
    promise: Promise<'js>,
    on_fulfilled: Function<'js>,
    on_rejected: Function<'js>,
) -> Result<Value<'js>> {
    promise
        .then()?
        .call((This(promise.clone()), on_fulfilled, on_rejected))
}

enum StartAlgorithm<'js> {
    ReturnUndefined,
    Function {
        f: Function<'js>,
        underlying_source: Object<'js>,
    },
}

#[derive(Trace)]
enum PullAlgorithm<'js> {
    ReturnPromiseUndefined,
    Function {
        f: Function<'js>,
        underlying_source: Object<'js>,
    },
}

#[derive(Trace)]
enum CancelAlgorithm<'js> {
    ReturnPromiseUndefined,
    Function {
        f: Function<'js>,
        underlying_source: Object<'js>,
    },
}

#[derive(Trace)]
struct ReadableStreamReadRequest<'js> {
    chunk_steps: Function<'js>,
    close_steps: Function<'js>,
    error_steps: Function<'js>,
}
