use std::{borrow::BorrowMut, collections::VecDeque};

use crate::modules::events::abort_signal::AbortSignal;
use llrt_utils::{
    bytes::ObjectBytes,
    error_messages::{ERROR_MSG_NOT_ARRAY_BUFFER, ERROR_MSG_NOT_ARRAY_BUFFER_VIEW},
    object::ObjectExt,
    result::ResultExt,
};
use rquickjs::{
    atom::PredefinedAtom,
    class::Trace,
    function::Constructor,
    prelude::{List, This},
    ArrayBuffer, Class, Ctx, Error, Exception, FromJs, Function, IntoJs, Object, Promise, Result,
    TypedArray, Undefined, Value,
};

use simd_json::prelude::ArrayTrait;

use super::{writeable::WriteableStream, ReadableWritablePair};

#[rquickjs::class]
#[derive(Trace)]
pub struct ReadableStream<'js> {
    controller: Option<ReadableStreamController<'js>>,
    disturbed: bool,
    state: ReadableStreamState,
    reader: Option<ReadableStreamReader<'js>>,
    stored_error: Option<Value<'js>>,
}

struct ReadableStreamInner {}

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
                ReadableStreamByteController::set_up_readable_byte_stream_controller_from_underlying_source(
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
    fn from(async_iterable: Value<'js>) -> Class<'js, Self> {
        unimplemented!()
    }

    // readonly attribute boolean locked;
    #[qjs(get)]
    fn locked(stream: This<Class<'js, Self>>) -> bool {
        unimplemented!()
    }

    // Promise<undefined> cancel(optional any reason);
    async fn cancel(stream: This<Class<'js, Self>>, reason: Value<'js>) -> Result<()> {
        unimplemented!()
    }

    // ReadableStreamReader getReader(optional ReadableStreamGetReaderOptions options = {});
    fn get_reader(
        stream: This<Class<'js, Self>>,
        options: Option<ReadableStreamGetReaderOptions>,
    ) -> ReadableStreamReader {
        unimplemented!()
    }

    // ReadableStream pipeThrough(ReadableWritablePair transform, optional StreamPipeOptions options = {});
    fn pipe_through(
        stream: This<Class<'js, Self>>,
        transform: ReadableWritablePair<'js>,
        options: Option<StreamPipeOptions<'js>>,
    ) -> Class<'js, ReadableStream<'js>> {
        unimplemented!()
    }

    // Promise<undefined> pipeTo(WritableStream destination, optional StreamPipeOptions options = {});
    async fn pipe_to(
        stream: This<Class<'js, Self>>,
        destination: Class<'js, WriteableStream>,
        options: Option<StreamPipeOptions<'js>>,
    ) -> Result<()> {
        unimplemented!()
    }

    // sequence<ReadableStream> tee();
    fn tee(stream: This<Class<'js, Self>>) -> List<(Class<'js, Self>, Class<'js, Self>)> {
        unimplemented!()
    }
}

impl<'js> ReadableStream<'js> {
    fn readable_stream_error(&mut self, e: Value) {
        unimplemented!()
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
            Some(ReadableStreamReader::ReadableStreamDefaultReader { ref read_requests, .. }) => read_requests.len(),
            _ => panic!("readable_stream_get_num_read_requests called without checking readable_stream_has_default_reader")
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
            Some(ReadableStreamReader::ReadableStreamBYOBReader { ref read_into_requests, .. }) => read_into_requests.len(),
            _ => panic!("readable_stream_get_num_read_requests called without checking readable_stream_has_byob_reader")
        }
    }

    fn readable_stream_fulfill_read_request(
        stream: Class<'js, Self>,
        chunk: ObjectBytes<'js>,
        done: bool,
    ) -> Result<()> {
        let mut stream = stream.borrow_mut();

        // Assert: ! ReadableStreamHasDefaultReader(stream) is true.
        // Let reader be stream.[[reader]].
        let read_requests = match stream.reader {
            Some(ReadableStreamReader::ReadableStreamDefaultReader { ref mut read_requests, ..}) => read_requests,
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
        let mut stream = stream.borrow_mut();
        // Assert: ! ReadableStreamHasBYOBReader(stream) is true.
        // Let reader be stream.[[reader]].
        let read_into_requests = match stream.reader {
            Some(ReadableStreamReader::ReadableStreamBYOBReader { ref mut read_into_requests, .. }) => read_into_requests,
            _ => panic!("ReadableStreamFulfillReadIntoRequest called on stream that doesn't satisfy ReadableStreamHasDefaultReader")
        };

        // Let readIntoRequest be reader.[[readIntoRequests]][0].
        // Remove readIntoRequest from reader.[[readIntoRequests]].
        let read_into_request = read_into_requests
            .pop_front()
            .expect("ReadableStreamFulfillReadIntoRequest called with empty readIntoRequests");

        if done {
            // If done is true, perform readIntoRequest’s close steps, given chunk.
            read_into_request.close_steps.call(())?;
        } else {
            // Otherwise, perform readIntoRequest’s chunk steps, given chunk.
            read_into_request.chunk_steps.call((chunk,))?;
        }

        Ok(())
    }

    fn readable_stream_close(stream: Class<'js, Self>) -> Result<()> {
        let mut stream = stream.borrow_mut();
        // Set stream.[[state]] to "closed".
        stream.state = ReadableStreamState::Closed;
        // Let reader be stream.[[reader]].
        let reader = match &stream.reader {
            // If reader is undefined, return.
            None => return Ok(()),
            Some(reader) => reader,
        };
        // Resolve reader.[[closedPromise]] with undefined.
        reader.generic().resolve_closed_promise.call((Undefined,))?;

        // If reader implements ReadableStreamDefaultReader,
        // Let readRequests be reader.[[readRequests]].
        if let ReadableStreamReader::ReadableStreamDefaultReader { read_requests, .. } = reader {
            // For each readRequest of readRequests,
            for read_request in read_requests {
                // Perform readRequest’s close steps.
                read_request.close_steps.call(())?;
            }
        };

        Ok(())
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
    auto_allocate_chunk_size: Option<u64>,
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

enum SizeAlgorithm<'js> {
    AlwaysOne,
    SizeFunction(Function<'js>),
}

struct ReadableStreamGetReaderOptions {
    mode: Option<ReadableStreamReaderMode>,
}

impl<'js> FromJs<'js> for ReadableStreamGetReaderOptions {
    fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
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
    BYOB,
}

impl<'js> FromJs<'js> for ReadableStreamReaderMode {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let str = value
            .as_string()
            .ok_or(Error::new_from_js(ty_name, "String"))?;

        match str.to_string()?.as_str() {
            "byob" => Ok(Self::BYOB),
            _ => Err(Error::new_from_js(ty_name, "ReadableStreamReaderMode")),
        }
    }
}

pub struct ReadableStreamGenericReader<'js> {
    resolve_closed_promise: Function<'js>,
    closed_promise: Promise<'js>,
    stream: Class<'js, ReadableStream<'js>>,
}

impl<'js> Trace<'js> for ReadableStreamGenericReader<'js> {
    fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
        self.resolve_closed_promise.trace(tracer);
        self.closed_promise.trace(tracer);
        self.stream.trace(tracer);
    }
}

// typedef (ReadableStreamDefaultController or ReadableByteStreamController) ReadableStreamController;
#[derive(Trace)]
#[rquickjs::class]
enum ReadableStreamReader<'js> {
    ReadableStreamDefaultReader {
        generic: ReadableStreamGenericReader<'js>,
        read_requests: VecDeque<ReadableStreamReadRequest<'js>>,
    },
    ReadableStreamBYOBReader {
        generic: ReadableStreamGenericReader<'js>,
        read_into_requests: VecDeque<ReadableStreamReadIntoRequest<'js>>,
    },
}

impl<'js> ReadableStreamReader<'js> {
    fn generic(&self) -> &ReadableStreamGenericReader<'js> {
        match self {
            ReadableStreamReader::ReadableStreamDefaultReader { generic, .. } => generic,
            ReadableStreamReader::ReadableStreamBYOBReader { generic, .. } => generic,
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

#[derive(Trace)]
enum ReadableStreamController<'js> {
    ReadableStreamDefaultController(ReadableStreamDefaultController),
    ReadableStreamByteController(Class<'js, ReadableStreamByteController<'js>>),
}

#[derive(Trace)]
struct ReadableStreamDefaultController {}

impl ReadableStreamDefaultController {
    fn set_up_readable_stream_default_controller_from_underlying_source<'js>(
        stream: Class<'js, ReadableStream<'js>>,
        underlying_source: Option<UnderlyingSource<'js>>,
        high_water_mark: f64,
        size_algorithm: SizeAlgorithm<'js>,
    ) -> Result<()> {
        unimplemented!()
    }

    // readonly attribute unrestricted double? desiredSize;
    fn desired_size(&self) -> Option<f64> {
        unimplemented!()
    }

    // undefined close();
    fn close() {
        unimplemented!()
    }

    // undefined enqueue(optional any chunk);
    fn enqueue(chunk: Value) {
        unimplemented!()
    }

    // undefined error(optional any e);
    fn error(e: Value) {
        unimplemented!()
    }
}

#[rquickjs::class]
#[derive(Trace, Default)]
struct ReadableStreamByteController<'js> {
    auto_allocate_chunk_size: Option<u64>,
    #[qjs(get)]
    byob_request: Option<ReadableStreamBYOBRequest<'js>>,
    cancel_algorithm: Option<Function<'js>>,
    close_requested: bool,
    pull_again: bool,
    pull_algorithm: Option<PullAlgorithm<'js>>,
    pulling: bool,
    pending_pull_intos: VecDeque<PullIntoDescriptor<'js>>,
    queue: VecDeque<ReadableByteStreamQueueEntry<'js>>,
    queue_total_size: usize,
    started: bool,
    strategy_hwm: usize,
    stream: Option<Class<'js, ReadableStream<'js>>>,
}

impl<'js> ReadableStreamByteController<'js> {
    // SetUpReadableByteStreamControllerFromUnderlyingSource
    fn set_up_readable_byte_stream_controller_from_underlying_source(
        ctx: &Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
        underlying_source: Option<UnderlyingSource<'js>>,
        high_water_mark: usize,
    ) -> Result<()> {
        let controller = Self::default();

        let (start_algorithm, pull_algorithm, cancel_algorithm, auto_allocate_chunk_size) =
            if let Some(underlying_source) = underlying_source {
                (
                    // If underlyingSourceDict["start"] exists, then set startAlgorithm to an algorithm which returns the result of invoking underlyingSourceDict["start"] with argument list
                    // « controller » and callback this value underlyingSource.
                    underlying_source
                        .start
                        .map(|f| StartAlgorithm::Function {
                            f,
                            underlying_source: underlying_source.js.clone(),
                        })
                        .unwrap_or(StartAlgorithm::ReturnUndefined),
                    // If underlyingSourceDict["pull"] exists, then set pullAlgorithm to an algorithm which returns the result of invoking underlyingSourceDict["pull"] with argument list
                    // « controller » and callback this value underlyingSource.
                    underlying_source
                        .pull
                        .map(|f| PullAlgorithm::Function {
                            f,
                            underlying_source: underlying_source.js,
                        })
                        .unwrap_or(PullAlgorithm::ReturnPromiseUndefined),
                    // If underlyingSourceDict["cancel"] exists, then set cancelAlgorithm to an algorithm which takes an argument reason and returns the result of invoking underlyingSourceDict["cancel"] with argument list
                    // « reason » and callback this value underlyingSource.
                    underlying_source.cancel,
                    // Let autoAllocateChunkSize be underlyingSourceDict["autoAllocateChunkSize"], if it exists, or undefined otherwise.
                    underlying_source.auto_allocate_chunk_size,
                )
            } else {
                (
                    StartAlgorithm::ReturnUndefined,
                    PullAlgorithm::ReturnPromiseUndefined,
                    None,
                    None,
                )
            };

        // If autoAllocateChunkSize is 0, then throw a TypeError exception.
        if auto_allocate_chunk_size == Some(0) {
            return Err(Exception::throw_type(
                ctx,
                "autoAllocateChunkSize must be greater than 0",
            ));
        }

        Self::set_up_readable_byte_stream_controller(
            ctx.clone(),
            stream,
            Class::instance(ctx.clone(), controller)?,
            start_algorithm,
            pull_algorithm,
            cancel_algorithm,
            high_water_mark,
            auto_allocate_chunk_size,
        )
    }

    fn set_up_readable_byte_stream_controller(
        ctx: Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
        controller: Class<'js, ReadableStreamByteController<'js>>,
        start_algorithm: StartAlgorithm<'js>,
        pull_algorithm: PullAlgorithm<'js>,
        cancel_algorithm: Option<Function<'js>>,
        high_water_mark: usize,
        auto_allocate_chunk_size: Option<u64>,
    ) -> Result<()> {
        {
            let mut controller = controller.borrow_mut();

            controller.stream = Some(stream.clone());

            // Set controller.[[pullAgain]] and controller.[[pulling]] to false.
            controller.pull_again = false;
            controller.pulling = false;

            // Set controller.[[byobRequest]] to null.
            controller.byob_request = None;

            // Perform ! ResetQueue(controller).
            controller.reset_queue();

            // Set controller.[[closeRequested]] and controller.[[started]] to false.
            controller.close_requested = false;
            controller.started = false;

            // Set controller.[[strategyHWM]] to highWaterMark.
            controller.strategy_hwm = high_water_mark;

            // Set controller.[[pullAlgorithm]] to pullAlgorithm.
            controller.pull_algorithm = Some(pull_algorithm);

            // Set controller.[[cancelAlgorithm]] to cancelAlgorithm.
            controller.cancel_algorithm = cancel_algorithm;

            // Set controller.[[autoAllocateChunkSize]] to autoAllocateChunkSize.
            controller.auto_allocate_chunk_size = auto_allocate_chunk_size;

            // Set controller.[[pendingPullIntos]] to a new empty list.
            controller.pending_pull_intos.clear();
        }

        stream.borrow_mut().controller = Some(
            ReadableStreamController::ReadableStreamByteController(controller.clone()),
        );

        // Let startResult be the result of performing startAlgorithm.
        let start_result: Value = match start_algorithm {
            StartAlgorithm::ReturnUndefined => Value::new_undefined(ctx.clone()),
            StartAlgorithm::Function {
                f,
                underlying_source,
            } => f.call((This(underlying_source), controller.clone()))?,
        };

        // Let startPromise be a promise resolved with startResult.
        let start_promise = promise_resolved_with(&ctx, start_result)?;

        let _ = upon_promise(
            start_promise,
            // Upon fulfillment of startPromise,
            Function::new(ctx.clone(), {
                let ctx = ctx.clone();
                let controller = controller.clone();
                let stream = stream.clone();
                move || {
                    // Set controller.[[started]] to true.
                    controller.borrow_mut().started = true;
                    // Perform ! ReadableByteStreamControllerCallPullIfNeeded(controller).
                    Self::readable_byte_stream_controller_call_pull_if_needed(
                        controller.clone(),
                        ctx.clone(),
                        stream.clone(),
                    )
                }
            })?,
            // Upon rejection of startPromise with reason r,
            Function::new(ctx.clone(), {
                let controller = controller.clone();
                move |r: Value| {
                    // Perform ! ReadableByteStreamControllerError(controller, r).
                    Self::readable_byte_stream_controller_error(controller.clone(), r)
                }
            })?,
        )?;

        Ok(())
    }

    fn readable_byte_stream_controller_call_pull_if_needed(
        controller: Class<'js, Self>,
        ctx: Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
    ) -> Result<()> {
        {
            let mut controller = controller.borrow_mut();
            // Let shouldPull be ! ReadableByteStreamControllerShouldCallPull(controller).
            let should_pull = controller.readable_byte_stream_controller_should_call_pull(&stream);

            // If shouldPull is false, return.
            if !should_pull {
                return Ok(());
            }

            // If controller.[[pulling]] is true,
            if controller.pulling {
                // Set controller.[[pullAgain]] to true.
                controller.pull_again = true;

                // Return.
                return Ok(());
            }

            // Set controller.[[pulling]] to true.
            controller.pulling = true;
        }

        // Let pullPromise be the result of performing controller.[[pullAlgorithm]].
        let pull_promise = match controller.borrow().pull_algorithm {
            None => {
                panic!("pull algorithm used after ReadableByteStreamControllerClearAlgorithms")
            },
            Some(PullAlgorithm::ReturnPromiseUndefined) => {
                promise_resolved_with(&ctx, Value::new_undefined(ctx.clone()))?
            },
            Some(PullAlgorithm::Function {
                ref f,
                ref underlying_source,
            }) => f.call((This(underlying_source.clone()), controller.clone()))?,
        };

        upon_promise(
            pull_promise,
            Function::new(ctx.clone(), {
                let ctx = ctx.clone();
                let stream = stream.clone();
                let controller = controller.clone();
                move || {
                    let mut controller_mut = controller.borrow_mut();
                    controller_mut.pulling = false;
                    if controller_mut.pull_again {
                        controller_mut.pull_again = false;
                        drop(controller_mut);
                        Self::readable_byte_stream_controller_call_pull_if_needed(
                            controller.clone(),
                            ctx.clone(),
                            stream.clone(),
                        )
                        .unwrap();
                    };
                }
            })?,
            Function::new(ctx.clone(), {
                let controller = controller.clone();
                move |e: Value<'js>| {
                    Self::readable_byte_stream_controller_error(controller.clone(), e);
                }
            })?,
        )?;

        Ok(())
    }

    fn readable_byte_stream_controller_should_call_pull(
        &self,
        stream: &Class<'js, ReadableStream<'js>>,
    ) -> bool {
        // Let stream be controller.[[stream]].
        match stream.borrow().state {
            ReadableStreamState::Readable => {},
            // If stream.[[state]] is not "readable", return false.
            _ => return false,
        }

        // If controller.[[closeRequested]] is true, return false.
        if self.close_requested {
            return false;
        }

        // If controller.[[started]] is false, return false.
        if !self.started {
            return false;
        }

        {
            let stream = stream.borrow();

            // If ! ReadableStreamHasDefaultReader(stream) is true and ! ReadableStreamGetNumReadRequests(stream) > 0, return true.
            if stream.readable_stream_has_default_reader()
                && stream.readable_stream_get_num_read_requests() > 0
            {
                return true;
            }

            // If ! ReadableStreamHasBYOBReader(stream) is true and ! ReadableStreamGetNumReadIntoRequests(stream) > 0, return true.
            if stream.readable_stream_has_byob_reader()
                && stream.readable_stream_get_num_read_into_requests() > 0
            {
                return true;
            }
        }

        // Let desiredSize be ! ReadableByteStreamControllerGetDesiredSize(controller).
        let desired_size = self.readable_byte_stream_controller_get_desired_size();

        // Assert: desiredSize is not null.
        if desired_size.expect("desired_size must not be null") > 0 {
            // If desiredSize > 0, return true.
            return true;
        }

        // Return false.
        false
    }

    fn readable_byte_stream_controller_error(controller: Class<'js, Self>, e: Value) {
        // Let stream be controller.[[stream]].
        let stream = controller.borrow().stream.clone();
        let stream = match stream {
            Some(stream) if stream.borrow().state == ReadableStreamState::Readable => stream,
            // If stream.[[state]] is not "readable", return.
            _ => return,
        };

        {
            let mut controller = controller.borrow_mut();
            // Perform ! ReadableByteStreamControllerClearPendingPullIntos(controller).
            controller.readable_byte_stream_controller_clear_pending_pull_intos();

            // Perform ! ResetQueue(controller).
            controller.reset_queue();

            // Perform ! ReadableByteStreamControllerClearAlgorithms(controller).
            controller.readable_byte_stream_controller_clear_algorithms();
        }

        // Perform ! ReadableStreamError(stream, e).
        stream.borrow_mut().readable_stream_error(e);
    }

    fn readable_byte_stream_controller_clear_pending_pull_intos(&mut self) {
        // Perform ! ReadableByteStreamControllerInvalidateBYOBRequest(controller).
        self.readable_byte_stream_controller_invalidate_byob_request();

        // Set controller.[[pendingPullIntos]] to a new empty list.
        self.pending_pull_intos.clear();
    }

    fn readable_byte_stream_controller_invalidate_byob_request(&mut self) {
        let byob_request = match self.byob_request {
            // If controller.[[byobRequest]] is null, return.
            None => return,
            Some(ref mut byob_request) => byob_request,
        };
        byob_request.controller = None;
        byob_request.view = None;

        self.byob_request = None;
    }

    fn readable_byte_stream_controller_clear_algorithms(&mut self) {
        self.pull_algorithm = None;
        self.cancel_algorithm = None;
    }

    fn readable_byte_stream_controller_get_desired_size(&self) -> Option<usize> {
        // Let state be controller.[[stream]].[[state]].
        match self
            .stream
            .as_ref()
            .expect("ReadableByteStreamControllerGetDesiredSize called without stream")
            .borrow()
            .state
        {
            // If state is "errored", return null.
            ReadableStreamState::Errored => None,
            // If state is "closed", return 0.
            ReadableStreamState::Closed => Some(0),
            // Return controller.[[strategyHWM]] − controller.[[queueTotalSize]].
            _ => Some(self.strategy_hwm - self.queue_total_size),
        }
    }

    fn reset_queue(&mut self) {
        // Set container.[[queue]] to a new empty list.
        self.queue.clear();
        // Set container.[[queueTotalSize]] to 0.
        self.queue_total_size = 0;
    }

    fn readable_byte_stream_controller_close(
        controller: Class<'js, Self>,
        ctx: &Ctx<'js>,
    ) -> Result<()> {
        {
            let mut controller = controller.borrow_mut();
            // Let stream be controller.[[stream]].
            // If controller.[[closeRequested]] is true or stream.[[state]] is not "readable", return.
            if controller.close_requested
                || controller.stream.as_ref().map(|s| s.borrow().state)
                    != Some(ReadableStreamState::Readable)
            {
                return Ok(());
            }

            // If controller.[[queueTotalSize]] > 0,
            if controller.queue_total_size > 0 {
                // Set controller.[[closeRequested]] to true.
                controller.close_requested = true;
                // Return.
                return Ok(());
            }
        }

        // If controller.[[pendingPullIntos]] is not empty,
        // Let firstPendingPullInto be controller.[[pendingPullIntos]][0].
        if let Some(first_pending_pull_into) = controller.borrow().pending_pull_intos.front() {
            // If the remainder after dividing firstPendingPullInto’s bytes filled by firstPendingPullInto’s element size is not 0,
            if first_pending_pull_into.bytes_filled % first_pending_pull_into.element_size != 0 {
                // Let e be a new TypeError exception.
                let e: Value = ctx.eval(
                    r#"new TypeError("Insufficient bytes to fill elements in the given buffer")"#,
                )?;
                Self::readable_byte_stream_controller_error(controller.clone(), e.clone());
                return Err(ctx.throw(e));
            }
        }

        Ok(())
    }

    fn readable_byte_stream_controller_enqueue(
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
        chunk: ObjectBytes<'js>,
    ) -> Result<()> {
        {
            let controller = controller.borrow();
            // Let stream be controller.[[stream]].
            let stream = controller.stream.as_ref();

            // If controller.[[closeRequested]] is true or stream.[[state]] is not "readable", return.
            if controller.close_requested
                || stream.map(|s| s.borrow().state) != Some(ReadableStreamState::Readable)
            {
                return Ok(());
            }
        }

        // Let buffer be chunk.[[ViewedArrayBuffer]].
        // Let byteOffset be chunk.[[ByteOffset]].
        // Let byteLength be chunk.[[ByteLength]].
        let (buffer, byte_length, byte_offset) = chunk.get_array_buffer()?.unwrap();

        // If ! IsDetachedBuffer(buffer) is true, throw a TypeError exception.
        buffer.as_raw().ok_or(Exception::throw_type(
            ctx,
            "chunk's buffer is detached and so cannot be enqueued",
        ))?;

        // Let transferredBuffer be ? TransferArrayBuffer(buffer).
        let transferred_buffer = transfer_array_buffer(ctx.clone(), buffer)?;

        {
            let mut controller_mut = controller.borrow_mut();
            // If controller.[[pendingPullIntos]] is not empty,
            // Let firstPendingPullInto be controller.[[pendingPullIntos]][0].
            if let Some(first_pending_pull_into) = controller_mut.pending_pull_intos.front_mut() {
                // If ! IsDetachedBuffer(firstPendingPullInto’s buffer) is true, throw a TypeError exception.
                first_pending_pull_into
                    .buffer
                    .as_raw()
                    .ok_or(Exception::throw_type(
                        ctx,
                        "The BYOB request's buffer has been detached and so cannot be filled with an enqueued chunk",
                    ))?;

                let existing_buffer = first_pending_pull_into.buffer.clone();

                {
                    // Perform ! ReadableByteStreamControllerInvalidateBYOBRequest(controller).
                    controller_mut.readable_byte_stream_controller_invalidate_byob_request();

                    // Set firstPendingPullInto’s buffer to ! TransferArrayBuffer(firstPendingPullInto’s buffer).
                    controller_mut.pending_pull_intos[0].buffer =
                        transfer_array_buffer(ctx.clone(), existing_buffer)?;
                }

                // If firstPendingPullInto’s reader type is "none", perform ? ReadableByteStreamControllerEnqueueDetachedPullIntoToQueue(controller, firstPendingPullInto).
                let controller = controller.clone();
                if let PullIntoDescriptorReaderType::None =
                    controller_mut.pending_pull_intos[0].reader_type
                {
                    Self::readable_byte_stream_enqueue_detached_pull_into_to_queue(
                        ctx.clone(),
                        controller.clone(),
                        &controller_mut.pending_pull_intos[0],
                    )?;
                }
            }
        }

        let stream = controller.borrow().stream.clone().unwrap();

        if stream.borrow().readable_stream_has_default_reader() {
            // If ! ReadableStreamHasDefaultReader(stream) is true,
            // Perform ! ReadableByteStreamControllerProcessReadRequestsUsingQueue(controller).
            Self::readable_byte_stream_controller_process_read_requests_using_queue(
                ctx,
                controller.clone(),
            )?;

            // If ! ReadableStreamGetNumReadRequests(stream) is 0,
            if stream.borrow().readable_stream_get_num_read_requests() == 0 {
                // Perform ! ReadableByteStreamControllerEnqueueChunkToQueue(controller, transferredBuffer, byteOffset, byteLength).
                Self::readable_byte_stream_controller_enqueue_chunk_to_queue(
                    controller.clone(),
                    transferred_buffer,
                    byte_offset,
                    byte_length,
                )
            } else {
                // Otherwise,
                // If controller.[[pendingPullIntos]] is not empty,
                if !controller.borrow().pending_pull_intos.is_empty() {
                    // Perform ! ReadableByteStreamControllerShiftPendingPullInto(controller).
                    Self::readable_byte_stream_controller_shift_pending_pull_into(
                        controller.clone(),
                    );
                }

                // Let transferredView be ! Construct(%Uint8Array%, « transferredBuffer, byteOffset, byteLength »).
                let ctor: Constructor = ctx.globals().get(PredefinedAtom::Uint8Array)?;
                let transferred_view: ObjectBytes =
                    ctor.construct((transferred_buffer, byte_offset, byte_length))?;

                // Perform ! ReadableStreamFulfillReadRequest(stream, transferredView, false).
                ReadableStream::readable_stream_fulfill_read_request(
                    stream.clone(),
                    transferred_view,
                    false,
                )?
            }
        } else if stream.borrow().readable_stream_has_byob_reader() {
            // Otherwise, if ! ReadableStreamHasBYOBReader(stream) is true,
            // Perform ! ReadableByteStreamControllerEnqueueChunkToQueue(controller, transferredBuffer, byteOffset, byteLength).
            Self::readable_byte_stream_controller_enqueue_chunk_to_queue(
                controller.clone(),
                transferred_buffer,
                byte_offset,
                byte_length,
            );
            // Perform ! ReadableByteStreamControllerProcessPullIntoDescriptorsUsingQueue(controller).
            Self::readable_byte_stream_controller_process_pull_into_descriptors_using_queue(
                ctx,
                controller.clone(),
            )?;
        } else {
            // Otherwise,
            // Perform ! ReadableByteStreamControllerEnqueueChunkToQueue(controller, transferredBuffer, byteOffset, byteLength).
            Self::readable_byte_stream_controller_enqueue_chunk_to_queue(
                controller.clone(),
                transferred_buffer,
                byte_offset,
                byte_length,
            );
        }

        // Perform ! ReadableByteStreamControllerCallPullIfNeeded(controller).
        Self::readable_byte_stream_controller_call_pull_if_needed(
            controller,
            ctx.clone(),
            stream.clone(),
        )
    }

    fn readable_byte_stream_enqueue_detached_pull_into_to_queue(
        ctx: Ctx<'js>,
        controller: Class<'js, Self>,
        pull_into_descriptor: &PullIntoDescriptor<'js>,
    ) -> Result<()> {
        // If pullIntoDescriptor’s bytes filled > 0, perform ? ReadableByteStreamControllerEnqueueClonedChunkToQueue(controller, pullIntoDescriptor’s buffer, pullIntoDescriptor’s byte offset, pullIntoDescriptor’s bytes filled).
        if pull_into_descriptor.bytes_filled > 0 {
            Self::readable_byte_stream_controller_enqueue_cloned_chunk_to_queue(
                ctx,
                controller.clone(),
                pull_into_descriptor.buffer.clone(),
                pull_into_descriptor.byte_offset,
                pull_into_descriptor.bytes_filled,
            )?;
        }

        // Perform ! ReadableByteStreamControllerShiftPendingPullInto(controller).
        Self::readable_byte_stream_controller_shift_pending_pull_into(controller);

        Ok(())
    }

    fn readable_byte_stream_controller_process_read_requests_using_queue(
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
    ) -> Result<()> {
        let stream = controller.borrow().stream.clone().expect(
            "ReadableByteStreamControllerProcessReadRequestsUsingQueue called without a stream",
        );
        let mut stream = stream.borrow_mut();

        // Let reader be controller.[[stream]].[[reader]].
        let read_requests = match stream.reader {
            Some(ReadableStreamReader::ReadableStreamDefaultReader { ref mut read_requests, .. }) => {read_requests},
            _ => panic!("ReadableByteStreamControllerProcessReadRequestsUsingQueue must be called with a stream that has a reader implementing ReadableStreamDefaultReader"),
        };

        // While reader.[[readRequests]] is not empty,
        while !read_requests.is_empty() {
            // If controller.[[queueTotalSize]] is 0, return.
            if controller.borrow().queue_total_size == 0 {
                return Ok(());
            }

            // Let readRequest be reader.[[readRequests]][0].
            // Remove readRequest from reader.[[readRequests]].
            let read_request = read_requests.pop_front().unwrap();
            // Perform ! ReadableByteStreamControllerFillReadRequestFromQueue(controller, readRequest).
            Self::readable_byte_stream_controller_fill_read_request_from_queue(
                ctx,
                controller.clone(),
                read_request,
            )?;
        }

        Ok(())
    }

    fn readable_byte_stream_controller_shift_pending_pull_into(
        controller: Class<'js, Self>,
    ) -> PullIntoDescriptor<'js> {
        // Let descriptor be controller.[[pendingPullIntos]][0].
        // Remove descriptor from controller.[[pendingPullIntos]].
        // Return descriptor.
        controller
            .borrow_mut()
            .pending_pull_intos
            .pop_front()
            .expect(
                "ReadableByteStreamControllerShiftPendingPullInto called on empty pendingPullIntos",
            )
    }

    fn readable_byte_stream_controller_enqueue_chunk_to_queue(
        controller: Class<'js, Self>,
        buffer: ArrayBuffer<'js>,
        byte_offset: usize,
        byte_length: usize,
    ) {
        let mut controller = controller.borrow_mut();
        let len = buffer.len();
        // Append a new readable byte stream queue entry with buffer buffer, byte offset byteOffset, and byte length byteLength to controller.[[queue]].
        controller.queue.push_back(ReadableByteStreamQueueEntry {
            buffer,
            byte_offset,
            byte_length,
        });

        // Set controller.[[queueTotalSize]] to controller.[[queueTotalSize]] + byteLength.
        controller.queue_total_size += len;
    }

    fn readable_byte_stream_controller_process_pull_into_descriptors_using_queue(
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
    ) -> Result<()> {
        let controller_mut = controller.borrow_mut();
        // While controller.[[pendingPullIntos]] is not empty,
        while !controller_mut.pending_pull_intos.is_empty() {
            // If controller.[[queueTotalSize]] is 0, return.
            if controller_mut.queue_total_size == 0 {
                return Ok(());
            }

            // Let pullIntoDescriptor be controller.[[pendingPullIntos]][0].
            let pull_into_descriptor = &controller_mut.pending_pull_intos[0];

            // If ! ReadableByteStreamControllerFillPullIntoDescriptorFromQueue(controller, pullIntoDescriptor) is true,
            if Self::readable_byte_stream_controller_fill_pull_into_descriptor_from_queue(
                controller.clone(),
                pull_into_descriptor,
            ) {
                // Perform ! ReadableByteStreamControllerShiftPendingPullInto(controller).
                Self::readable_byte_stream_controller_shift_pending_pull_into(controller.clone());

                // Perform ! ReadableByteStreamControllerCommitPullIntoDescriptor(controller.[[stream]], pullIntoDescriptor).
                Self::readable_byte_stream_controller_commit_pull_into_descriptor(
                    ctx.clone(),
                    controller_mut.stream.clone().expect("ReadableByteStreamControllerProcessPullIntoDescriptorsUsingQueue called without stream"),
                    pull_into_descriptor,
                )?;
            }
        }
        Ok(())
    }

    fn readable_byte_stream_controller_enqueue_cloned_chunk_to_queue(
        ctx: Ctx<'js>,
        controller: Class<'js, Self>,
        buffer: ArrayBuffer<'js>,
        byte_offset: usize,
        byte_length: usize,
    ) -> Result<()> {
        // Let cloneResult be CloneArrayBuffer(buffer, byteOffset, byteLength, %ArrayBuffer%).
        let clone_result = match ArrayBuffer::new_copy(
            ctx.clone(),
            buffer.as_bytes().expect(
                "ReadableByteStreamControllerEnqueueClonedChunkToQueue called on detached buffer",
            ),
        ) {
            Ok(clone_result) => clone_result,
            Err(err) => {
                Self::readable_byte_stream_controller_error(controller, ctx.catch());
                return Err(err);
            },
        };

        Self::readable_byte_stream_controller_enqueue_chunk_to_queue(
            controller,
            clone_result,
            byte_offset,
            byte_length,
        );

        Ok(())
    }

    fn readable_byte_stream_controller_fill_read_request_from_queue(
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        let entry = {
            let mut controller = controller.borrow_mut();
            // Assert: controller.[[queueTotalSize]] > 0.
            // Let entry be controller.[[queue]][0].
            // Remove entry from controller.[[queue]].
            let entry = controller.queue.pop_front().expect(
                "ReadableByteStreamControllerFillReadRequestFromQueue called with empty queue",
            );

            // Set controller.[[queueTotalSize]] to controller.[[queueTotalSize]] − entry’s byte length.
            controller.queue_total_size -= entry.byte_length;

            entry
        };

        // Perform ! ReadableByteStreamControllerHandleQueueDrain(controller).
        Self::readable_byte_stream_controller_handle_queue_drain(controller, ctx)?;

        // Let view be ! Construct(%Uint8Array%, « entry’s buffer, entry’s byte offset, entry’s byte length »).
        let ctor: Constructor = ctx.globals().get(PredefinedAtom::Uint8Array)?;
        let view: TypedArray<u8> =
            ctor.construct((entry.buffer, entry.byte_offset, entry.byte_length))?;

        // Perform readRequest’s chunk steps, given view.
        read_request.chunk_steps.call((view,))?;

        Ok(())
    }

    fn readable_byte_stream_controller_fill_pull_into_descriptor_from_queue(
        controller: Class<'js, Self>,
        pull_into_descriptor: &PullIntoDescriptor<'js>,
    ) -> bool {
        // Let maxBytesToCopy be min(controller.[[queueTotalSize]], pullIntoDescriptor’s byte length − pullIntoDescriptor’s bytes filled).
        let max_bytes_to_copy = std::cmp::min(
            controller.borrow().queue_total_size,
            pull_into_descriptor.byte_length - pull_into_descriptor.bytes_filled,
        );

        // Let maxBytesFilled be pullIntoDescriptor’s bytes filled + maxBytesToCopy.
        let max_bytes_filled = pull_into_descriptor.bytes_filled + max_bytes_to_copy;

        // Let totalBytesToCopyRemaining be maxBytesToCopy.
        let mut total_bytes_to_copy_remaining = max_bytes_to_copy;

        // Let ready be false.
        let mut ready = false;

        // Let remainderBytes be the remainder after dividing maxBytesFilled by pullIntoDescriptor’s element size.
        let remainder_bytes = max_bytes_filled % pull_into_descriptor.element_size;

        // Let maxAlignedBytes be maxBytesFilled − remainderBytes.
        let max_aligned_bytes = max_bytes_filled - remainder_bytes;

        // If maxAlignedBytes ≥ pullIntoDescriptor’s minimum fill,
        if max_aligned_bytes > pull_into_descriptor.minimum_fill {
            // Set totalBytesToCopyRemaining to maxAlignedBytes − pullIntoDescriptor’s bytes filled.
            total_bytes_to_copy_remaining = max_aligned_bytes - pull_into_descriptor.bytes_filled;
            // Set ready to true.
            ready = true
        }

        // Let queue be controller.[[queue]].
        let queue = &mut controller.borrow_mut().queue;
        // While totalBytesToCopyRemaining > 0,
        while total_bytes_to_copy_remaining > 0 {
            // Let headOfQueue be queue[0].
            let head_of_queue = queue.front_mut().expect("empty queue with bytes to copy");
            // Let bytesToCopy be min(totalBytesToCopyRemaining, headOfQueue’s byte length).
            let bytes_to_copy =
                std::cmp::min(total_bytes_to_copy_remaining, head_of_queue.byte_length);
            // Let destStart be pullIntoDescriptor’s byte offset + pullIntoDescriptor’s bytes filled.
            let dest_start = pull_into_descriptor.byte_offset + pull_into_descriptor.bytes_filled;
            // Perform ! CopyDataBlockBytes(pullIntoDescriptor’s buffer.[[ArrayBufferData]], destStart, headOfQueue’s buffer.[[ArrayBufferData]], headOfQueue’s byte offset, bytesToCopy).
            copy_data_block_bytes(
                pull_into_descriptor.buffer.clone(),
                dest_start,
                head_of_queue.buffer.clone(),
                head_of_queue.byte_offset,
                bytes_to_copy,
            );
            if head_of_queue.byte_length == bytes_to_copy {
                // If headOfQueue’s byte length is bytesToCopy,
                // Remove queue[0].
                queue.pop_front();
            } else {
                // Otherwise,
                // Set headOfQueue’s byte offset to headOfQueue’s byte offset + bytesToCopy.
                head_of_queue.byte_offset += bytes_to_copy;
                // Set headOfQueue’s byte length to headOfQueue’s byte length − bytesToCopy.
                head_of_queue.byte_length -= bytes_to_copy
            }

            // Set controller.[[queueTotalSize]] to controller.[[queueTotalSize]] − bytesToCopy.
            controller.borrow_mut().queue_total_size -= bytes_to_copy;

            // Set totalBytesToCopyRemaining to totalBytesToCopyRemaining − bytesToCopy.
            total_bytes_to_copy_remaining -= bytes_to_copy
        }

        ready
    }

    fn readable_byte_stream_controller_commit_pull_into_descriptor(
        ctx: Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
        pull_into_descriptor: &PullIntoDescriptor<'js>,
    ) -> Result<()> {
        // Let done be false.
        let mut done = false;
        // If stream.[[state]] is "closed",
        if let ReadableStreamState::Closed = stream.borrow().state {
            // Set done to true.
            done = true
        }

        // Let filledView be ! ReadableByteStreamControllerConvertPullIntoDescriptor(pullIntoDescriptor).
        let filled_view = Self::readable_byte_stream_controller_convert_pull_into_descriptor(
            ctx,
            pull_into_descriptor,
        )?;

        if let PullIntoDescriptorReaderType::Default = pull_into_descriptor.reader_type {
            // If pullIntoDescriptor’s reader type is "default",
            // Perform ! ReadableStreamFulfillReadRequest(stream, filledView, done).
            ReadableStream::readable_stream_fulfill_read_request(stream, filled_view, done)?
        } else {
            // Otherwise,
            // Perform ! ReadableStreamFulfillReadIntoRequest(stream, filledView, done).
            ReadableStream::readable_stream_fulfill_read_into_request(stream, filled_view, done)?
        }

        Ok(())
    }

    fn readable_byte_stream_controller_handle_queue_drain(
        controller: Class<'js, Self>,
        ctx: &Ctx<'js>,
    ) -> Result<()> {
        let mut controller_mut = controller.borrow_mut();
        let stream = controller_mut
            .stream
            .clone()
            .expect("ReadableByteStreamControllerHandleQueueDrain called without stream");

        // If controller.[[queueTotalSize]] is 0 and controller.[[closeRequested]] is true,
        if controller_mut.queue_total_size == 0 && controller_mut.close_requested {
            // Perform ! ReadableByteStreamControllerClearAlgorithms(controller).
            controller_mut.readable_byte_stream_controller_clear_algorithms();
            // Perform ! ReadableStreamClose(controller.[[stream]]).
            ReadableStream::readable_stream_close(stream)?;
        } else {
            drop(controller_mut);
            // Otherwise,
            // Perform ! ReadableByteStreamControllerCallPullIfNeeded(controller).
            Self::readable_byte_stream_controller_call_pull_if_needed(
                controller,
                ctx.clone(),
                stream,
            )?
        }

        Ok(())
    }

    fn readable_byte_stream_controller_convert_pull_into_descriptor(
        ctx: Ctx<'js>,
        pull_into_descriptor: &PullIntoDescriptor<'js>,
    ) -> Result<ObjectBytes<'js>> {
        // Let bytesFilled be pullIntoDescriptor’s bytes filled.
        let bytes_filled = pull_into_descriptor.bytes_filled;
        // Let elementSize be pullIntoDescriptor’s element size.
        let element_size = pull_into_descriptor.element_size;
        // Let buffer be ! TransferArrayBuffer(pullIntoDescriptor’s buffer).
        let buffer = transfer_array_buffer(ctx.clone(), pull_into_descriptor.buffer.clone());
        // Return ! Construct(pullIntoDescriptor’s view constructor, « buffer, pullIntoDescriptor’s byte offset, bytesFilled ÷ elementSize »).
        let view: Object = pull_into_descriptor.view_constructor.construct((
            buffer,
            pull_into_descriptor.byte_offset,
            bytes_filled / element_size,
        ))?;
        ObjectBytes::from(&ctx, &view)
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

fn copy_data_block_bytes<'js>(
    to_block: ArrayBuffer,
    to_index: usize,
    from_block: ArrayBuffer,
    from_index: usize,
    count: usize,
) {
    unimplemented!()
}

#[rquickjs::methods(rename_all = "camelCase")]
impl<'js> ReadableStreamByteController<'js> {
    // readonly attribute unrestricted double? desiredSize;
    #[qjs(get)]
    fn desired_size(&self) -> Option<usize> {
        self.readable_byte_stream_controller_get_desired_size()
    }

    // undefined close();
    fn close(controller: This<Class<'js, Self>>, ctx: Ctx<'js>) -> Result<()> {
        {
            let controller = controller.borrow();
            // If this.[[closeRequested]] is true, throw a TypeError exception.
            if controller.close_requested {
                return Err(Exception::throw_type(&ctx, "close() called more than once"));
            }
            match controller.stream {
                Some(ref stream) if stream.borrow().state == ReadableStreamState::Readable => {},
                // If this.[[stream]].[[state]] is not "readable", throw a TypeError exception.
                _ => {
                    return Err(Exception::throw_type(
                        &ctx,
                        "close() called when stream is not readable",
                    ));
                },
            }
        }

        // Perform ? ReadableByteStreamControllerClose(this).
        Self::readable_byte_stream_controller_close(controller.0, &ctx)
    }

    // undefined enqueue(ArrayBufferView chunk);
    fn enqueue(
        controller: This<Class<'js, Self>>,
        ctx: Ctx<'js>,
        chunk: Object<'js>,
    ) -> Result<()> {
        let chunk = if let Some(chunk) = ObjectBytes::from_array_buffer(&chunk)? {
            chunk
        } else {
            return Err(Exception::throw_message(
                &ctx,
                ERROR_MSG_NOT_ARRAY_BUFFER_VIEW,
            ));
        };

        let (array_buffer, byte_length, _) = chunk
            .get_array_buffer()?
            .ok_or(ERROR_MSG_NOT_ARRAY_BUFFER)
            .or_throw(&ctx)?;

        // If chunk.[[ByteLength]] is 0, throw a TypeError exception.
        if byte_length == 0 {
            return Err(Exception::throw_type(
                &ctx,
                "chunk must have non-zero byteLength",
            ));
        }

        // If chunk.[[ViewedArrayBuffer]].[[ArrayBufferByteLength]] is 0, throw a TypeError exception.
        if array_buffer.is_empty() {
            return Err(Exception::throw_type(
                &ctx,
                "chunk must have non-zero buffer byteLength",
            ));
        }

        // If this.[[closeRequested]] is true, throw a TypeError exception.
        if controller.borrow().close_requested {
            return Err(Exception::throw_type(&ctx, "stream is closed or draining"));
        }

        // If this.[[stream]].[[state]] is not "readable", throw a TypeError exception.
        if controller
            .borrow()
            .stream
            .as_ref()
            .map(|stream| stream.borrow().state)
            != Some(ReadableStreamState::Readable)
        {
            return Err(Exception::throw_type(
                &ctx,
                "The stream is not in the readable state and cannot be enqueued to",
            ));
        }

        // Return ? ReadableByteStreamControllerEnqueue(this, chunk).
        Self::readable_byte_stream_controller_enqueue(&ctx, controller.0, chunk)
    }

    // undefined error(optional any e);
    fn error(controller: This<Class<'js, Self>>, e: Value<'js>) {
        // Perform ! ReadableByteStreamControllerError(this, e).
        Self::readable_byte_stream_controller_error(controller.0, e)
    }
}

#[derive(Trace, Clone)]
#[rquickjs::class]
struct ReadableStreamBYOBRequest<'js> {
    view: Option<Value<'js>>,
    controller: Option<Class<'js, ReadableStreamByteController<'js>>>,
}

fn promise_resolved_with<'js>(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Promise<'js>> {
    let (promise, resolve, _) = Promise::new(ctx)?;
    resolve.call((value,))?;
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

struct PullIntoDescriptor<'js> {
    buffer: ArrayBuffer<'js>,
    buffer_byte_length: u64,
    byte_offset: usize,
    byte_length: usize,
    bytes_filled: usize,
    minimum_fill: usize,
    element_size: usize,
    view_constructor: Constructor<'js>,
    reader_type: PullIntoDescriptorReaderType,
}

impl<'js> Trace<'js> for PullIntoDescriptor<'js> {
    fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
        self.buffer.trace(tracer);
        self.view_constructor.trace(tracer);
    }
}

#[derive(Trace)]
enum PullIntoDescriptorReaderType {
    Default,
    Byob,
    None,
}

struct ReadableByteStreamQueueEntry<'js> {
    buffer: ArrayBuffer<'js>,
    byte_offset: usize,
    byte_length: usize,
}

impl<'js> Trace<'js> for ReadableByteStreamQueueEntry<'js> {
    fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
        self.buffer.trace(tracer)
    }
}

#[derive(Trace)]
struct ReadableStreamReadRequest<'js> {
    chunk_steps: Function<'js>,
    close_steps: Function<'js>,
    error_steps: Function<'js>,
}

#[derive(Trace)]
struct ReadableStreamReadIntoRequest<'js> {
    chunk_steps: Function<'js>,
    close_steps: Function<'js>,
    error_steps: Function<'js>,
}
