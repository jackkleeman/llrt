use std::{
    cell::{Cell, OnceCell},
    rc::Rc,
};

use crate::modules::events::abort_signal::AbortSignal;
use byob_reader::ReadableStreamReadIntoRequest;
use byte_controller::ReadableStreamByteController;
use default_controller::ReadableStreamDefaultController;
use iterator::{IteratorKind, IteratorRecord, ReadableStreamAsyncIterator};
use llrt_utils::{
    bytes::ObjectBytes, error_messages::ERROR_MSG_ARRAY_BUFFER_DETACHED, result::ResultExt,
};
use rquickjs::{
    atom::PredefinedAtom,
    class::{JsClass, OwnedBorrow, OwnedBorrowMut, Trace, Tracer},
    prelude::{List, MutFn, OnceFn, Opt, This},
    ArrayBuffer, Class, Ctx, Error, Exception, FromJs, Function, IntoAtom, IntoJs, Object, Promise,
    Result, Type, Value,
};

use super::{writeable::WriteableStream, ReadableWritablePair};

mod byob_reader;
mod byte_controller;
mod count_queueing_strategy;
mod default_controller;
mod default_reader;
mod iterator;
mod tee;

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

#[derive(Debug, Trace, Clone, Copy, PartialEq, Eq)]
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
        underlying_source: Opt<Undefined<Object<'js>>>,
        queuing_strategy: Opt<Undefined<QueuingStrategy<'js>>>,
    ) -> Result<Class<'js, Self>> {
        // If underlyingSource is missing, set it to null.
        let underlying_source = Null(underlying_source.0);

        // Let underlyingSourceDict be underlyingSource, converted to an IDL value of type UnderlyingSource.
        let underlying_source_dict = match underlying_source {
            Null(None) | Null(Some(Undefined(None))) => UnderlyingSource::default(),
            Null(Some(Undefined(Some(ref obj)))) => UnderlyingSource::from_object(obj.clone())?,
        };

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
        let queuing_strategy = queuing_strategy.0.and_then(|qs| qs.0);

        match underlying_source_dict.r#type {
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
                    QueuingStrategy::extract_high_water_mark(&ctx, queuing_strategy, 0.0)? as usize;

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
                let size_algorithm =
                    QueuingStrategy::extract_size_algorithm(queuing_strategy.as_ref());

                // Let highWaterMark be ? ExtractHighWaterMark(strategy, 1).
                let high_water_mark =
                    QueuingStrategy::extract_high_water_mark(&ctx, queuing_strategy, 1.0)?;

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
    fn from(ctx: Ctx<'js>, async_iterable: Value<'js>) -> Result<Class<'js, Self>> {
        // Return ? ReadableStreamFromIterable(asyncIterable).
        Self::readable_stream_from_iterable(&ctx, async_iterable)
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
        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(
            stream
                .controller
                .clone()
                .expect("Cancel called on stream without a controller"),
        );
        let reader = stream.reader_mut();
        let (promise, _, _, _) = Self::readable_stream_cancel(
            ctx.clone(),
            stream.0,
            controller,
            reader,
            reason.0.unwrap_or(Value::new_undefined(ctx)),
        )?;
        Ok(promise)
    }

    // ReadableStreamReader getReader(optional ReadableStreamGetReaderOptions options = {});
    fn get_reader(
        ctx: Ctx<'js>,
        stream: This<OwnedBorrowMut<'js, Self>>,
        options: Opt<Option<ReadableStreamGetReaderOptions>>,
    ) -> Result<ReadableStreamReader<'js>> {
        // If options["mode"] does not exist, return ? AcquireReadableStreamDefaultReader(this).
        let reader = match options.0 {
            None | Some(None | Some(ReadableStreamGetReaderOptions { mode: None })) => {
                let (_, reader) = ReadableStreamReader::acquire_readable_stream_default_reader(
                    ctx.clone(),
                    stream.0,
                )?;
                ReadableStreamReader::ReadableStreamDefaultReader(reader)
            },
            // Return ? AcquireReadableStreamBYOBReader(this).
            Some(Some(ReadableStreamGetReaderOptions {
                mode: Some(ReadableStreamReaderMode::Byob),
            })) => ReadableStreamReader::ReadableStreamBYOBReader(
                ReadableStreamReader::acquire_readable_stream_byob_reader(ctx.clone(), stream.0)?,
            ),
        };

        Ok(reader)
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
    fn tee(
        ctx: Ctx<'js>,
        stream: This<OwnedBorrowMut<'js, Self>>,
    ) -> Result<List<(Class<'js, Self>, Class<'js, Self>)>> {
        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(
            stream
                .controller
                .clone()
                .expect("ReadableStream tee caleld without controller"),
        );
        // Return ? ReadableStreamTee(this, false).
        Ok(List(Self::readable_stream_tee(
            ctx, stream.0, controller, false,
        )?))
    }

    #[qjs(rename = iterator::SymbolAsyncIterator)]
    fn async_iterate(
        ctx: Ctx<'js>,
        stream: This<OwnedBorrowMut<'js, Self>>,
    ) -> Result<Class<'js, ReadableStreamAsyncIterator<'js>>> {
        Self::values(ctx, stream, Opt(None))
    }

    fn values(
        ctx: Ctx<'js>,
        stream: This<OwnedBorrowMut<'js, Self>>,
        arg: Opt<Object<'js>>,
    ) -> Result<Class<'js, ReadableStreamAsyncIterator<'js>>> {
        // Let reader be ? AcquireReadableStreamDefaultReader(stream).
        let (stream, reader) =
            ReadableStreamReader::acquire_readable_stream_default_reader(ctx.clone(), stream.0)?;

        // Let preventCancel be args[0]["preventCancel"].
        let prevent_cancel = match arg.0 {
            None => false,
            Some(arg) => match arg.get_optional("preventCancel")? {
                Some(true) => true,
                _ => false,
            },
        };

        let controller = stream
            .controller
            .clone()
            .expect("ReadableStream iterator values called without controller");
        let stream = stream.into_inner();

        ReadableStreamAsyncIterator::new(ctx, stream, controller, reader, prevent_cancel)
    }
}

impl<'js> ReadableStream<'js> {
    fn readable_stream_error(
        ctx: &Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, Self>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        e: Value<'js>,
    ) -> Result<()> {
        // Set stream.[[state]] to "errored".
        stream.state = ReadableStreamState::Errored;
        // Set stream.[[storedError]] to e.
        stream.stored_error = Some(e.clone());
        // Let reader be stream.[[reader]].
        let mut reader = match reader {
            // If reader is undefined, return.
            None => return Ok(()),
            Some(reader) => reader,
        };

        match reader {
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(r) => {
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
                ReadableStreamDefaultReader::readable_stream_default_reader_error_read_requests(
                    stream, controller, r, e,
                )?;
                Ok(())
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
        ctx: &Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, Self>,
        mut controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        // Let reader be stream.[[reader]].
        mut reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        chunk: Value<'js>,
        done: bool,
    ) -> Result<(
        OwnedBorrowMut<'js, Self>,
        ReadableStreamControllerOwnedBorrowMut<'js>,
        Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    )> {
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
            (stream, controller, reader) =
                read_request.close_steps(ctx, stream, controller, reader)?;
        } else {
            // Otherwise, perform readRequest’s chunk steps, given chunk.
            (stream, controller, reader) =
                read_request.chunk_steps(stream, controller, reader, chunk)?;
        }

        Ok((stream, controller, reader))
    }

    fn readable_stream_fulfill_read_into_request(
        &mut self,
        ctx: &Ctx<'js>,
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
            read_into_request.close_steps(chunk.into_js(ctx)?)?;
        } else {
            // Otherwise, perform readIntoRequest’s chunk steps, given chunk.
            read_into_request.chunk_steps(chunk.into_js(ctx)?)?;
        }

        Ok(())
    }

    fn readable_stream_close(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, Self>,
        mut controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        // Let reader be stream.[[reader]].
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    ) -> Result<(
        OwnedBorrowMut<'js, Self>,
        ReadableStreamControllerOwnedBorrowMut<'js>,
        Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    )> {
        // Set stream.[[state]] to "closed".
        stream.state = ReadableStreamState::Closed;
        let mut reader = match reader {
            // If reader is undefined, return.
            None => return Ok((stream, controller, reader)),
            Some(reader) => reader,
        };
        match &mut reader {
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(r) => {
                // Resolve reader.[[closedPromise]] with undefined.
                r.generic
                    .resolve_closed_promise
                    .as_ref()
                    .expect("ReadableStreamClose called without resolution function")
                    .call((Value::new_undefined(ctx.clone()),))?;

                // If reader implements ReadableStreamDefaultReader,
                // Let readRequests be reader.[[readRequests]].
                // Set reader.[[readRequests]] to an empty list.
                let read_requests = r.read_requests.split_off(0);

                let mut reader = Some(reader);

                // For each readRequest of readRequests,
                for read_request in read_requests {
                    // Perform readRequest’s close steps.
                    (stream, controller, reader) =
                        read_request.close_steps(&ctx, stream, controller, reader)?;
                }

                Ok((stream, controller, reader))
            },
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(r) => {
                r.generic
                    .resolve_closed_promise
                    .as_ref()
                    .expect("ReadableStreamClose called without resolution function")
                    .call((Value::new_undefined(ctx),))?;

                Ok((stream, controller, Some(reader)))
            },
        }
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
        reader: Option<&mut ReadableStreamReaderOwnedBorrowMut<'js>>,
        read_request: ReadableStreamReadRequest<'js>,
    ) {
        match reader.expect("ReadableStreamAddReadRequest called without reader") {
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(_) => {
                panic!("ReadableStreamAddReadRequest called with byob reader")
            },
            ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(ref mut r) => {
                r.read_requests.push_back(read_request)
            },
        }
    }

    fn readable_stream_cancel(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        mut reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        reason: Value<'js>,
    ) -> Result<(
        Promise<'js>,
        OwnedBorrowMut<'js, Self>,
        ReadableStreamControllerOwnedBorrowMut<'js>,
        Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    )> {
        // Set stream.[[disturbed]] to true.
        stream.disturbed = true;

        match stream.state {
            // If stream.[[state]] is "closed", return a promise resolved with undefined.
            ReadableStreamState::Closed => Ok((
                promise_resolved_with(&ctx, Ok(Value::new_undefined(ctx.clone())))?,
                stream,
                controller,
                reader,
            )),
            // If stream.[[state]] is "errored", return a promise rejected with stream.[[storedError]].
            ReadableStreamState::Errored => Ok((
                promise_rejected_with(
                    &ctx,
                    stream
                        .stored_error
                        .clone()
                        .expect("ReadableStream in errored state without a stored error"),
                )?,
                stream,
                controller,
                reader,
            )),
            ReadableStreamState::Readable => {
                // Perform ! ReadableStreamClose(stream).
                (stream, controller, reader) =
                    ReadableStream::readable_stream_close(ctx.clone(), stream, controller, reader)?;
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
                        read_into_request.close_steps(Value::new_undefined(ctx.clone()))?
                    }
                }

                // Let sourceCancelPromise be ! stream.[[controller]].[[CancelSteps]](reason).
                let (source_cancel_promise, stream, controller, reader) =
                    controller.cancel_steps(&ctx, stream, reader, reason)?;

                // Return the result of reacting to sourceCancelPromise with a fulfillment step that returns undefined.
                let promise = source_cancel_promise.then()?.call((
                    This(source_cancel_promise.clone()),
                    Function::new(ctx.clone(), move || {
                        Value::new_undefined(ctx.clone())
                    })?,
                ))?;

                Ok((
                    promise,
                    OwnedBorrowMut::from_class(stream),
                    controller,
                    reader.map(ReadableStreamReaderOwnedBorrowMut::from_class),
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

    // CreateReadableStream(startAlgorithm, pullAlgorithm, cancelAlgorithm[, highWaterMark, [, sizeAlgorithm]]) performs the following steps:
    fn create_readable_stream(
        ctx: Ctx<'js>,
        start_algorithm: StartAlgorithm<'js>,
        pull_algorithm: PullAlgorithm<'js>,
        cancel_algorithm: CancelAlgorithm<'js>,
        high_water_mark: Option<f64>,
        size_algorithm: Option<SizeAlgorithm<'js>>,
    ) -> Result<Class<'js, Self>> {
        // If highWaterMark was not passed, set it to 1.
        let high_water_mark = high_water_mark.unwrap_or(1.0);

        // If sizeAlgorithm was not passed, set it to an algorithm that returns 1.
        let size_algorithm = size_algorithm.unwrap_or(SizeAlgorithm::AlwaysOne);

        // Assert: ! IsNonNegativeNumber(highWaterMark) is true.

        // Let stream be a new ReadableStream.
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

        // Let controller be a new ReadableStreamDefaultController.
        let controller =
            OwnedBorrowMut::from_class(Class::instance(ctx.clone(), Default::default())?);

        // Perform ? SetUpReadableStreamDefaultController(stream, controller, startAlgorithm, pullAlgorithm, cancelAlgorithm, highWaterMark, sizeAlgorithm).
        ReadableStreamDefaultController::set_up_readable_stream_default_controller(
            ctx,
            controller,
            OwnedBorrowMut::from_class(stream_class.clone()),
            start_algorithm,
            pull_algorithm,
            cancel_algorithm,
            high_water_mark,
            size_algorithm,
        )?;

        // Return stream.
        Ok(stream_class)
    }

    fn readable_stream_from_iterable(
        ctx: &Ctx<'js>,
        async_iterable: Value<'js>,
    ) -> Result<Class<'js, Self>> {
        let stream: Rc<OnceCell<Class<'js, Self>>> = Rc::new(OnceCell::new());

        // Let iteratorRecord be ? GetIterator(asyncIterable, async).
        let mut iterator_record =
            IteratorRecord::get_iterator(ctx, async_iterable, IteratorKind::Async)?;
        let iterator = iterator_record.iterator.clone();

        // Let startAlgorithm be an algorithm that returns undefined.
        let start_algorithm = StartAlgorithm::ReturnUndefined;

        // Let pullAlgorithm be the following steps:
        let pull_algorithm = {
            let ctx = ctx.clone();
            let stream = stream.clone();
            move |controller: Class<'js, ReadableStreamDefaultController<'js>>| {
                // Let nextResult be IteratorNext(iteratorRecord).
                let next_result: Result<Object<'js>> = iterator_record.iterator_next(&ctx, None);
                let next_promise = match next_result {
                    // If nextResult is an abrupt completion, return a promise rejected with nextResult.[[Value]].
                    Err(Error::Exception) => {
                        return promise_rejected_with(&ctx, ctx.catch());
                    },
                    Err(err) => return Err(err),
                    // Let nextPromise be a promise resolved with nextResult.[[Value]].
                    Ok(next_result) => promise_resolved_with(&ctx, Ok(next_result.into_inner()))?,
                };

                // Return the result of reacting to nextPromise with the following fulfillment steps, given iterResult:
                next_promise.then()?.call((
                This(next_promise.clone()),
                Function::new(ctx.clone(), OnceFn::new({
                    let ctx = ctx.clone();
                    let stream = stream.clone();
                    move |iter_result: Value<'js>| {

                    let iter_result = match iter_result.into_object() {
                        // If Type(iterResult) is not Object, throw a TypeError.
                        None => {
                            let e: Value = ctx
                                .eval(r#"new TypeError("The promise returned by the iterator.next() method must fulfill with an object")"#)?;
                            return Err(ctx.throw(e));
                        }
                        Some(iter_result) => iter_result,
                    };

                    // Let done be ? IteratorComplete(iterResult).
                    let done = IteratorRecord::iterator_complete(&iter_result)?;

                    let mut stream = OwnedBorrowMut::from_class(stream.get().cloned().expect("ReadableStreamFromIterable pull steps called with uninitialised stream"));
                    let controller = OwnedBorrowMut::from_class(controller);

                    // If done is true:
                    if done {
                        // Perform ! ReadableStreamDefaultControllerClose(stream.[[controller]]).
                        ReadableStreamDefaultController::readable_stream_default_controller_close(ctx.clone(), stream, controller)
                    } else {
                        // Let value be ? IteratorValue(iterResult).
                        let value = IteratorRecord::iterator_value(&iter_result)?;

                        let reader = stream.reader_mut();

                        // Perform ! ReadableStreamDefaultControllerEnqueue(stream.[[controller]], value).
                        ReadableStreamDefaultController::readable_stream_default_controller_enqueue(ctx.clone(), controller, stream, reader, value)
                    }
                }}))?,
            ))
            }
        };

        // Let cancelAlgorithm be the following steps, given reason:
        let cancel_algorithm = {
            let ctx = ctx.clone();
            move |reason: Value<'js>| {
                // Let iterator be iteratorRecord.[[Iterator]].

                // Let returnMethod be GetMethod(iterator, "return").
                let return_method: Function<'js> = match iterator.get(PredefinedAtom::Return) {
                    // If returnMethod is an abrupt completion, return a promise rejected with returnMethod.[[Value]].
                    Err(Error::Exception) => {
                        return promise_rejected_with(&ctx, ctx.catch());
                    },
                    Err(err) => return Err(err),
                    Ok(None) => {
                        // If returnMethod.[[Value]] is undefined, return a promise resolved with undefined.
                        return promise_resolved_with(&ctx, Ok(Value::new_undefined(ctx.clone())));
                    },
                    Ok(Some(return_method)) => return_method,
                };

                // Let returnResult be Call(returnMethod.[[Value]], iterator, « reason »).
                let return_result: Result<Value<'js>> =
                    return_method.call((This(iterator), reason));

                let return_result = match return_result {
                    // If returnResult is an abrupt completion, return a promise rejected with returnResult.[[Value]].
                    Err(Error::Exception) => {
                        return promise_rejected_with(&ctx, ctx.catch());
                    },
                    Err(err) => return Err(err),
                    Ok(return_result) => return_result,
                };

                // Let returnPromise be a promise resolved with returnResult.[[Value]].
                let return_promise = promise_resolved_with(&ctx, Ok(return_result))?;

                // Return the result of reacting to returnPromise with the following fulfillment steps, given iterResult:
                return_promise.then()?.call((
                This(return_promise.clone()),
                Function::new(
                    ctx.clone(),
                    OnceFn::new({
                        let ctx = ctx.clone();
                        move |iter_result: Value<'js>| {
                            // If Type(iterResult) is not Object, throw a TypeError.
                            if !iter_result.is_object() {
                                let e: Value = ctx
                                    .eval(r#"new TypeError("The promise returned by the iterator.next() method must fulfill with an object")"#)?;
                                return Err(ctx.throw(e));
                            }
                            // Return undefined.
                            return Ok(Value::new_undefined(ctx));
                        }
                    }),
                ),
            ))
            }
        };

        let s = ReadableStream::create_readable_stream(
            ctx.clone(),
            start_algorithm,
            PullAlgorithm::Function {
                f: Function::new(ctx.clone(), MutFn::new(pull_algorithm))?,
                underlying_source: Null(None),
            },
            CancelAlgorithm::Function {
                f: Function::new(ctx.clone(), OnceFn::new(cancel_algorithm))?,
                underlying_source: Null(None),
            },
            Some(0.0),
            None,
        )?;
        _ = stream.set(s.clone());
        Ok(s)
    }

    fn reader_mut(&mut self) -> Option<ReadableStreamReaderOwnedBorrowMut<'js>> {
        self.reader
            .clone()
            .map(ReadableStreamReaderOwnedBorrowMut::from_class)
    }
}

#[derive(Default)]
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

impl<'js> UnderlyingSource<'js> {
    fn from_object(obj: Object<'js>) -> Result<Self> {
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
        let typ = value.type_of();
        let str = match typ {
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

        match str.to_string()?.as_str() {
            "bytes" => Ok(Self::Bytes),
            _ => Err(Error::new_from_js(typ.as_str(), "ReadableStreamType")),
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
        this: Option<QueuingStrategy<'js>>,
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
    fn extract_size_algorithm(this: Option<&QueuingStrategy<'js>>) -> SizeAlgorithm<'js> {
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

    fn readable_stream_reader_generic_release(
        &mut self,
        ctx: &Ctx<'js>,
        stream: &mut ReadableStream<'js>,
        controller: &mut ReadableStreamControllerOwnedBorrowMut<'js>,
    ) -> Result<()> {
        // Let stream be reader.[[stream]].
        // Assert: stream is not undefined.

        // If stream.[[state]] is "readable", reject reader.[[closedPromise]] with a TypeError exception.
        if let ReadableStreamState::Readable = stream.state {
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
        controller.release_steps();

        // Set stream.[[reader]] to undefined.
        stream.reader = None;

        // Set reader.[[stream]] to undefined.
        self.stream = None;

        Ok(())
    }

    fn readable_stream_reader_generic_cancel(
        ctx: Ctx<'js>,
        // Let stream be reader.[[stream]].
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        reader: ReadableStreamReaderOwnedBorrowMut<'js>,
        reason: Value<'js>,
    ) -> Result<(
        Promise<'js>,
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        ReadableStreamControllerOwnedBorrowMut<'js>,
        ReadableStreamReaderOwnedBorrowMut<'js>,
    )> {
        // Return ! ReadableStreamCancel(stream, reason).
        let (promise, stream, controller, reader) =
            ReadableStream::readable_stream_cancel(ctx, stream, controller, Some(reader), reason)?;
        Ok((promise, stream, controller, reader.unwrap()))
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

enum ReadableStreamReaderOwnedBorrowMut<'js> {
    ReadableStreamDefaultReader(OwnedBorrowMut<'js, ReadableStreamDefaultReader<'js>>),
    ReadableStreamBYOBReader(OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>),
}

impl<'js> ReadableStreamReaderOwnedBorrowMut<'js> {
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

    fn into_default_reader(self) -> Option<OwnedBorrowMut<'js, ReadableStreamDefaultReader<'js>>> {
        match self {
            Self::ReadableStreamDefaultReader(r) => Some(r),
            Self::ReadableStreamBYOBReader(_) => None,
        }
    }

    // fn into_byob_reader(self) -> Option<OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>> {
    //     match self {
    //         Self::ReadableStreamBYOBReader(r) => Some(r),
    //         Self::ReadableStreamDefaultReader(_) => None,
    //     }
    // }
}

impl<'js> From<OwnedBorrowMut<'js, ReadableStreamDefaultReader<'js>>>
    for ReadableStreamReaderOwnedBorrowMut<'js>
{
    fn from(value: OwnedBorrowMut<'js, ReadableStreamDefaultReader<'js>>) -> Self {
        Self::ReadableStreamDefaultReader(value)
    }
}

impl<'js> From<OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>>
    for ReadableStreamReaderOwnedBorrowMut<'js>
{
    fn from(value: OwnedBorrowMut<'js, ReadableStreamBYOBReader<'js>>) -> Self {
        Self::ReadableStreamBYOBReader(value)
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
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        Class<'js, ReadableStreamDefaultReader<'js>>,
    )> {
        ReadableStreamDefaultReader::set_up_readable_stream_default_reader(&ctx, stream)
    }

    fn acquire_readable_stream_byob_reader(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
    ) -> Result<Class<'js, ReadableStreamBYOBReader<'js>>> {
        ReadableStreamBYOBReader::new(ctx, stream)
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

enum ReadableStreamControllerOwnedBorrowMut<'js> {
    ReadableStreamDefaultController(OwnedBorrowMut<'js, ReadableStreamDefaultController<'js>>),
    ReadableStreamByteController(OwnedBorrowMut<'js, ReadableStreamByteController<'js>>),
}

impl<'js> ReadableStreamControllerOwnedBorrowMut<'js> {
    fn from_class(class: ReadableStreamController<'js>) -> Self {
        match class {
            ReadableStreamController::ReadableStreamDefaultController(r) => {
                Self::ReadableStreamDefaultController(OwnedBorrowMut::from_class(r))
            },
            ReadableStreamController::ReadableStreamByteController(r) => {
                Self::ReadableStreamByteController(OwnedBorrowMut::from_class(r))
            },
        }
    }

    fn pull_steps(
        self,
        ctx: &Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        reader: ReadableStreamReaderOwnedBorrowMut<'js>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        match self {
            Self::ReadableStreamDefaultController(controller) => {
                ReadableStreamDefaultController::pull_steps(
                    ctx,
                    controller,
                    stream,
                    Some(reader),
                    read_request,
                )
            },
            Self::ReadableStreamByteController(controller) => {
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
        self,
        ctx: &Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        reason: Value<'js>,
    ) -> Result<(
        Promise<'js>,
        Class<'js, ReadableStream<'js>>,
        Self,
        Option<ReadableStreamReader<'js>>,
    )> {
        match self {
            Self::ReadableStreamDefaultController(controller) => {
                let (result, stream, controller, reader) =
                    ReadableStreamDefaultController::cancel_steps(
                        ctx, controller, stream, reader, reason,
                    )?;
                Ok((result, stream, controller.into(), reader))
            },
            Self::ReadableStreamByteController(controller) => {
                let (result, stream, controller, reader) =
                    ReadableStreamByteController::cancel_steps(
                        ctx, controller, stream, reader, reason,
                    )?;
                Ok((result, stream, controller.into(), reader))
            },
        }
    }

    fn release_steps(&mut self) {
        match self {
            Self::ReadableStreamDefaultController(c) => c.release_steps(),
            Self::ReadableStreamByteController(c) => c.release_steps(),
        }
    }

    fn into_default_controller(
        self,
    ) -> Option<OwnedBorrowMut<'js, ReadableStreamDefaultController<'js>>> {
        match self {
            Self::ReadableStreamDefaultController(c) => Some(c),
            Self::ReadableStreamByteController(_) => None,
        }
    }

    fn into_byte_controller(
        self,
    ) -> Option<OwnedBorrowMut<'js, ReadableStreamByteController<'js>>> {
        match self {
            Self::ReadableStreamByteController(c) => Some(c),
            Self::ReadableStreamDefaultController(_) => None,
        }
    }
}

impl<'js> From<OwnedBorrowMut<'js, ReadableStreamDefaultController<'js>>>
    for ReadableStreamControllerOwnedBorrowMut<'js>
{
    fn from(value: OwnedBorrowMut<'js, ReadableStreamDefaultController<'js>>) -> Self {
        Self::ReadableStreamDefaultController(value)
    }
}

impl<'js> From<OwnedBorrowMut<'js, ReadableStreamByteController<'js>>>
    for ReadableStreamControllerOwnedBorrowMut<'js>
{
    fn from(value: OwnedBorrowMut<'js, ReadableStreamByteController<'js>>) -> Self {
        Self::ReadableStreamByteController(value)
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
    then: impl FnOnce(Ctx<'js>, std::result::Result<Input, Value<'js>>) -> Result<Output> + 'js,
) -> Result<Promise<'js>> {
    let then = Rc::new(Cell::new(Some(then)));
    let then2 = then.clone();
    promise.then()?.call((
        This(promise.clone()),
        Function::new(
            ctx.clone(),
            OnceFn::new(move |ctx, input| {
                then.take()
                    .expect("Promise.then should only call either resolve or reject")(
                    ctx,
                    Ok(input),
                )
            }),
        ),
        Function::new(
            ctx,
            OnceFn::new(move |ctx, e: Value<'js>| {
                then2
                    .take()
                    .expect("Promise.then should only call either resolve or reject")(
                    ctx, Err(e)
                )
            }),
        ),
    ))
}

fn set_promise_is_handled_to_true<'js>(ctx: Ctx<'js>, promise: &Promise<'js>) -> Result<()> {
    promise.then()?.call((
        This(promise.clone()),
        Value::new_undefined(ctx.clone()),
        Function::new(ctx, || {}),
    ))
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

#[derive(Clone)]
enum StartAlgorithm<'js> {
    ReturnUndefined,
    Function {
        f: Function<'js>,
        underlying_source: Null<Undefined<Object<'js>>>,
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
        underlying_source: Null<Undefined<Object<'js>>>,
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
        underlying_source: Null<Undefined<Object<'js>>>,
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
            } => {
                let result: Result<Value> = f.call((This(underlying_source.clone()), reason));
                let promise = promise_resolved_with(&ctx, result);
                promise
            },
        }
    }
}

struct ReadableStreamReadRequest<'js> {
    chunk_steps: Box<
        dyn FnOnce(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                ReadableStreamControllerOwnedBorrowMut<'js>,
                Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
                Value<'js>,
            ) -> Result<(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                ReadableStreamControllerOwnedBorrowMut<'js>,
                Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
            )> + 'js,
    >,
    close_steps: Box<
        dyn FnOnce(
                &Ctx<'js>,
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                ReadableStreamControllerOwnedBorrowMut<'js>,
                Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
            ) -> Result<(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                ReadableStreamControllerOwnedBorrowMut<'js>,
                Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
            )> + 'js,
    >,
    error_steps: Box<
        dyn FnOnce(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                ReadableStreamControllerOwnedBorrowMut<'js>,
                Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
                Value<'js>,
            ) -> Result<(
                OwnedBorrowMut<'js, ReadableStream<'js>>,
                ReadableStreamControllerOwnedBorrowMut<'js>,
                Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
            )> + 'js,
    >,
    trace: Box<dyn Fn(Tracer<'_, 'js>) + 'js>,
}

impl<'js> ReadableStreamReadRequest<'js> {
    fn chunk_steps(
        self,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        chunk: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        ReadableStreamControllerOwnedBorrowMut<'js>,
        Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    )> {
        let chunk_steps = self.chunk_steps;
        chunk_steps(stream, controller, reader, chunk)
    }

    fn close_steps(
        self,
        ctx: &Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        ReadableStreamControllerOwnedBorrowMut<'js>,
        Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    )> {
        let close_steps = self.close_steps;
        close_steps(ctx, stream, controller, reader)
    }

    fn error_steps(
        self,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        reason: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        ReadableStreamControllerOwnedBorrowMut<'js>,
        Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    )> {
        let error_steps = self.error_steps;
        error_steps(stream, controller, reader, reason)
    }
}

impl<'js> Trace<'js> for ReadableStreamReadRequest<'js> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        (self.trace)(tracer)
    }
}

struct ReadableStreamReadResult<'js> {
    value: Option<Value<'js>>,
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

/// Helper type for converting an option into null instead of undefined.
#[derive(Clone)]
struct Null<T>(pub Option<T>);

impl<'js, T: IntoJs<'js>> IntoJs<'js> for Null<T> {
    fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
        match self.0 {
            None => Ok(Value::new_null(ctx.clone())),
            Some(val) => val.into_js(ctx),
        }
    }
}

impl<'js, T: Trace<'js>> Trace<'js> for Null<T> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        self.0.trace(tracer)
    }
}

/// Helper type for treating an undefined value as None, but not null
#[derive(Clone)]
struct Undefined<T>(pub Option<T>);

impl<'js, T: FromJs<'js>> FromJs<'js> for Undefined<T> {
    fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        if value.type_of() == Type::Undefined {
            Ok(Self(None))
        } else {
            Ok(Self(Some(FromJs::from_js(ctx, value)?)))
        }
    }
}

impl<T> Default for Undefined<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<'js, T: Trace<'js>> Trace<'js> for Undefined<T> {
    fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
        self.0.trace(tracer)
    }
}

impl<'js, T: IntoJs<'js>> IntoJs<'js> for Undefined<T> {
    fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
        match self.0 {
            None => Ok(Value::new_undefined(ctx.clone())),
            Some(val) => val.into_js(ctx),
        }
    }
}

// the trait used elsewhere in this repo accepts null values as 'None', which causes many web platform tests to fail as they
// like to check that undefined is accepted and null isn't.
pub trait ObjectExt<'js> {
    fn get_optional<K: IntoAtom<'js> + Clone, V: FromJs<'js>>(&self, k: K) -> Result<Option<V>>;
}

impl<'js> ObjectExt<'js> for Object<'js> {
    fn get_optional<K: IntoAtom<'js> + Clone, V: FromJs<'js> + Sized>(
        &self,
        k: K,
    ) -> Result<Option<V>> {
        let value = self.get::<K, Value<'js>>(k)?;
        Ok(Undefined::from_js(self.ctx(), value)?.0)
    }
}

impl<'js> ObjectExt<'js> for Value<'js> {
    fn get_optional<K: IntoAtom<'js> + Clone, V: FromJs<'js>>(&self, k: K) -> Result<Option<V>> {
        if let Some(obj) = self.as_object() {
            return obj.get_optional(k);
        }
        Ok(None)
    }
}
