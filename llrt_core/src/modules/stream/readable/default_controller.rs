use std::collections::VecDeque;

use rquickjs::{
    class::{OwnedBorrow, OwnedBorrowMut, Trace},
    methods,
    prelude::{Opt, This},
    Class, Ctx, Error, Exception, Object, Promise, Result, Value,
};

use crate::modules::stream::{
    class_from_owned_borrow_mut,
    readable::{
        CancelAlgorithm, PullAlgorithm, ReadableStreamReader, ReadableStreamReaderOwnedBorrowMut,
        StartAlgorithm,
    },
    upon_promise,
};

use super::{
    promise_resolved_with, Null, ReadableStream, ReadableStreamController,
    ReadableStreamReadRequest, ReadableStreamState, SizeAlgorithm, Undefined, UnderlyingSource,
};

#[derive(Trace, Default)]
#[rquickjs::class]
pub(super) struct ReadableStreamDefaultController<'js> {
    cancel_algorithm: Option<CancelAlgorithm<'js>>,
    close_requested: bool,
    pull_again: bool,
    pull_algorithm: Option<PullAlgorithm<'js>>,
    pulling: bool,
    queue: VecDeque<ValueWithSize<'js>>,
    queue_total_size: f64,
    started: bool,
    strategy_hwm: f64,
    strategy_size_algorithm: Option<SizeAlgorithm<'js>>,
    stream: Option<Class<'js, ReadableStream<'js>>>,
}

#[derive(Trace)]
struct ValueWithSize<'js> {
    value: Value<'js>,
    size: f64,
}

impl<'js> ReadableStreamDefaultController<'js> {
    pub(super) fn set_up_readable_stream_default_controller_from_underlying_source(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        underlying_source: Null<Undefined<Object<'js>>>,
        underlying_source_dict: UnderlyingSource<'js>,
        high_water_mark: f64,
        size_algorithm: SizeAlgorithm<'js>,
    ) -> Result<()> {
        // Let controller be a new ReadableStreamDefaultController.
        let controller = Self::default();

        let (start_algorithm, pull_algorithm, cancel_algorithm) = (
            // If underlyingSourceDict["start"] exists, then set startAlgorithm to an algorithm which returns the result of invoking underlyingSourceDict["start"] with argument list
            // « controller » and callback this value underlyingSource.
            underlying_source_dict
                .start
                .map(|f| StartAlgorithm::Function {
                    f,
                    underlying_source: underlying_source.clone(),
                })
                .unwrap_or(StartAlgorithm::ReturnUndefined),
            // If underlyingSourceDict["pull"] exists, then set pullAlgorithm to an algorithm which returns the result of invoking underlyingSourceDict["pull"] with argument list
            // « controller » and callback this value underlyingSource.
            underlying_source_dict
                .pull
                .map(|f| PullAlgorithm::Function {
                    f,
                    underlying_source: underlying_source.clone(),
                })
                .unwrap_or(PullAlgorithm::ReturnPromiseUndefined),
            // If underlyingSourceDict["cancel"] exists, then set cancelAlgorithm to an algorithm which takes an argument reason and returns the result of invoking underlyingSourceDict["cancel"] with argument list
            // « reason » and callback this value underlyingSource.
            underlying_source_dict
                .cancel
                .map(|f| CancelAlgorithm::Function {
                    f,
                    underlying_source,
                })
                .unwrap_or(CancelAlgorithm::ReturnPromiseUndefined),
        );

        let controller = OwnedBorrowMut::from_class(Class::instance(ctx.clone(), controller)?);

        // Perform ? SetUpReadableStreamDefaultController(stream, controller, startAlgorithm, pullAlgorithm, cancelAlgorithm, highWaterMark, sizeAlgorithm).
        Self::set_up_readable_stream_default_controller(
            ctx.clone(),
            controller,
            stream,
            start_algorithm,
            pull_algorithm,
            cancel_algorithm,
            high_water_mark,
            size_algorithm,
        )
    }

    pub(super) fn set_up_readable_stream_default_controller(
        ctx: Ctx<'js>,
        mut controller: OwnedBorrowMut<'js, Self>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        start_algorithm: StartAlgorithm<'js>,
        pull_algorithm: PullAlgorithm<'js>,
        cancel_algorithm: CancelAlgorithm<'js>,
        high_water_mark: f64,
        size_algorithm: SizeAlgorithm<'js>,
    ) -> Result<()> {
        let (stream_class, mut stream) = class_from_owned_borrow_mut(stream);
        // Set controller.[[stream]] to stream.
        controller.stream = Some(stream_class.clone());

        // Perform ! ResetQueue(controller).
        controller.reset_queue();

        // Set controller.[[started]], controller.[[closeRequested]], controller.[[pullAgain]], and controller.[[pulling]] to false.
        controller.started = false;
        controller.close_requested = false;
        controller.pull_again = false;
        controller.pulling = false;

        // Set controller.[[strategySizeAlgorithm]] to sizeAlgorithm and controller.[[strategyHWM]] to highWaterMark.
        controller.strategy_size_algorithm = Some(size_algorithm);
        controller.strategy_hwm = high_water_mark;

        // Set controller.[[pullAlgorithm]] to pullAlgorithm.
        controller.pull_algorithm = Some(pull_algorithm);

        // Set controller.[[cancelAlgorithm]] to cancelAlgorithm.
        controller.cancel_algorithm = Some(cancel_algorithm);

        let (controller_class, controller) = class_from_owned_borrow_mut(controller);

        // Set stream.[[controller]] to controller.
        stream.controller = Some(ReadableStreamController::ReadableStreamDefaultController(
            controller_class.clone(),
        ));

        // Let startResult be the result of performing startAlgorithm. (This might throw an exception.)
        let (start_result, controller_class, stream_class) =
            Self::start_algorithm(ctx.clone(), controller, stream, start_algorithm)?;

        // Let startPromise be a promise resolved with startResult.
        let start_promise = promise_resolved_with(&ctx, Ok(start_result))?;

        let _ = upon_promise::<Value<'js>, _>(ctx.clone(), start_promise, {
            move |ctx, result| {
                let mut controller = OwnedBorrowMut::from_class(controller_class);
                let mut stream = OwnedBorrowMut::from_class(stream_class);
                let reader = stream.reader_mut();
                match result {
                    // Upon fulfillment of startPromise,
                    Ok(_) => {
                        // Set controller.[[started]] to true.
                        controller.started = true;
                        // Perform ! ReadableByteStreamControllerCallPullIfNeeded(controller).
                        Self::readable_stream_default_controller_call_pull_if_needed(
                            ctx, controller, stream, reader,
                        )?;
                    },
                    // Upon rejection of startPromise with reason r,
                    Err(r) => {
                        // Perform ! ReadableByteStreamControllerError(controller, r).
                        Self::readable_stream_default_controller_error(
                            &ctx, stream, controller, reader, r,
                        )?;
                    },
                }
                Ok(())
            }
        })?;

        Ok(())
    }

    fn reset_queue(&mut self) {
        // Set container.[[queue]] to a new empty list.
        self.queue.clear();
        // Set container.[[queueTotalSize]] to 0.
        self.queue_total_size = 0.0;
    }

    fn readable_stream_default_controller_call_pull_if_needed(
        ctx: Ctx<'js>,
        mut controller: OwnedBorrowMut<'js, Self>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    ) -> Result<(
        OwnedBorrowMut<'js, ReadableStream<'js>>,
        OwnedBorrowMut<'js, Self>,
        Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    )> {
        // Let shouldPull be ! ReadableStreamDefaultControllerShouldCallPull(controller).
        let should_pull = controller
            .readable_stream_default_controller_should_call_pull(&stream, reader.as_ref());

        // If shouldPull is false, return.
        if !should_pull {
            return Ok((stream, controller, reader));
        }

        // If controller.[[pulling]] is true,
        if controller.pulling {
            // Set controller.[[pullAgain]] to true.
            controller.pull_again = true;

            // Return.
            return Ok((stream, controller, reader));
        }

        // Set controller.[[pulling]] to true.
        controller.pulling = true;

        // Let pullPromise be the result of performing controller.[[pullAlgorithm]].
        let (pull_promise, controller, stream, reader) =
            Self::pull_algorithm(ctx.clone(), controller, stream, reader)?;

        upon_promise::<Value<'js>, _>(ctx.clone(), pull_promise, {
            let controller = controller.clone();
            let stream = stream.clone();
            move |ctx, result| {
                let mut controller = OwnedBorrowMut::from_class(controller);
                let mut stream = OwnedBorrowMut::from_class(stream);
                let reader = stream.reader_mut();
                match result {
                    // Upon fulfillment of pullPromise,
                    Ok(_) => {
                        // Set controller.[[pulling]] to false.
                        controller.pulling = false;
                        // If controller.[[pullAgain]] is true,
                        if controller.pull_again {
                            // Set controller.[[pullAgain]] to false.
                            controller.pull_again = false;
                            // Perform ! ReadableStreamDefaultControllerCallPullIfNeeded(controller).
                            Self::readable_stream_default_controller_call_pull_if_needed(
                                ctx, controller, stream, reader,
                            )?;
                        };
                        Ok(())
                    },
                    // Upon rejection of pullPromise with reason e,
                    Err(e) => {
                        // Perform ! ReadableStreamDefaultControllerError(controller, e).
                        Self::readable_stream_default_controller_error(
                            &ctx, stream, controller, reader, e,
                        )
                    },
                }
            }
        })?;

        Ok((
            OwnedBorrowMut::from_class(stream),
            OwnedBorrowMut::from_class(controller),
            reader.map(ReadableStreamReaderOwnedBorrowMut::from_class),
        ))
    }

    pub(super) fn readable_stream_default_controller_error(
        ctx: &Ctx<'js>,
        // Let stream be controller.[[stream]].
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, Self>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        e: Value<'js>,
    ) -> Result<()> {
        // If stream.[[state]] is not "readable", return.
        if stream.state != ReadableStreamState::Readable {
            return Ok(());
        }

        // Perform ! ResetQueue(controller).
        controller.reset_queue();

        // Perform ! ReadableStreamDefaultControllerClearAlgorithms(controller).
        controller.readable_stream_default_controller_clear_algorithms();

        // Perform ! ReadableStreamError(stream, e).
        ReadableStream::readable_stream_error(ctx, stream, controller.into(), reader, e)?;

        Ok(())
    }

    fn readable_stream_default_controller_should_call_pull(
        &self,
        stream: &ReadableStream<'js>,
        reader: Option<&ReadableStreamReaderOwnedBorrowMut<'js>>,
    ) -> bool {
        // Let stream be controller.[[stream]].
        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(controller) is false, return false.
        if !self.readable_stream_default_controller_can_close_or_enqueue(stream) {
            return false;
        }

        // If controller.[[started]] is false, return false.
        if !self.started {
            return false;
        }

        {
            // If ! IsReadableStreamLocked(stream) is true and ! ReadableStreamGetNumReadRequests(stream) > 0, return true.
            if stream.is_readable_stream_locked()
                && ReadableStream::readable_stream_get_num_read_requests(reader) > 0
            {
                return true;
            }
        }

        // Let desiredSize be ! ReadableStreamDefaultControllerGetDesiredSize(controller).
        let desired_size = self
            .readable_stream_default_controller_get_desired_size(stream)
            .0
            .expect(
            "desiredSize should not be null during ReadableStreamDefaultControllerShouldCallPull",
        );
        // If desiredSize > 0, return true.
        if desired_size > 0.0 {
            return true;
        }

        // Return false.
        false
    }

    fn readable_stream_default_controller_clear_algorithms(&mut self) {
        self.pull_algorithm = None;
        self.cancel_algorithm = None;
        self.strategy_size_algorithm = None;
    }

    fn readable_stream_default_controller_can_close_or_enqueue(
        &self,
        stream: &ReadableStream<'js>,
    ) -> bool {
        // Let state be controller.[[stream]].[[state]].
        // If controller.[[closeRequested]] is false and state is "readable", return true.
        if !self.close_requested && stream.state == ReadableStreamState::Readable {
            true
        } else {
            // Otherwise, return false.
            false
        }
    }

    fn readable_stream_default_controller_get_desired_size(
        &self,
        stream: &ReadableStream<'js>,
    ) -> Null<f64> {
        // Let state be controller.[[stream]].[[state]].
        match stream.state {
            // If state is "errored", return null.
            ReadableStreamState::Errored => Null(None),
            // If state is "closed", return 0.
            ReadableStreamState::Closed => Null(Some(0.0)),
            // Return controller.[[strategyHWM]] − controller.[[queueTotalSize]].
            ReadableStreamState::Readable => Null(Some(self.strategy_hwm - self.queue_total_size)),
        }
    }

    pub(super) fn readable_stream_default_controller_close(
        ctx: Ctx<'js>,
        // Let stream be controller.[[stream]].
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, Self>,
    ) -> Result<()> {
        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(controller) is false, return.
        if !controller.readable_stream_default_controller_can_close_or_enqueue(&mut stream) {
            return Ok(());
        }

        // Set controller.[[closeRequested]] to true.
        controller.close_requested = true;

        // If controller.[[queue]] is empty,
        if controller.queue.is_empty() {
            // Perform ! ReadableStreamDefaultControllerClearAlgorithms(controller).
            controller.readable_stream_default_controller_clear_algorithms();
            let reader = stream.reader_mut();
            // Perform ! ReadableStreamClose(stream).
            ReadableStream::readable_stream_close(ctx, stream, controller.into(), reader)?;
        }

        Ok(())
    }

    pub(super) fn readable_stream_default_controller_enqueue(
        ctx: Ctx<'js>,
        mut controller: OwnedBorrowMut<'js, Self>,
        // Let stream be controller.[[stream]].
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        chunk: Value<'js>,
    ) -> Result<()> {
        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(controller) is false, return.
        if !controller.readable_stream_default_controller_can_close_or_enqueue(&stream) {
            return Ok(());
        }

        // If ! IsReadableStreamLocked(stream) is true and ! ReadableStreamGetNumReadRequests(stream) > 0, perform ! ReadableStreamFulfillReadRequest(stream, chunk, false).
        if stream.is_readable_stream_locked()
            && ReadableStream::readable_stream_get_num_read_requests(reader.as_ref()) > 0
        {
            let mut c = controller.into();
            (stream, c, reader) = ReadableStream::readable_stream_fulfill_read_request(
                &ctx, stream, c, reader, chunk, false,
            )?;
            controller = c.into_default_controller().expect(
                "readable_stream_fulfill_read_request must return the same controller type",
            );
        } else {
            // Let result be the result of performing controller.[[strategySizeAlgorithm]], passing in chunk, and interpreting the result as a completion record.
            let (result, controller_class, stream_class, reader_class) =
                Self::strategy_size_algorithm(
                    ctx.clone(),
                    controller,
                    stream,
                    reader,
                    chunk.clone(),
                );

            controller = OwnedBorrowMut::from_class(controller_class);
            stream = OwnedBorrowMut::from_class(stream_class);
            reader = reader_class.map(ReadableStreamReaderOwnedBorrowMut::from_class);

            match result {
                // If result is an abrupt completion,
                Err(Error::Exception) => {
                    let err = ctx.catch();
                    // Perform ! ReadableStreamDefaultControllerError(controller, result.[[Value]]).
                    Self::readable_stream_default_controller_error(
                        &ctx,
                        stream,
                        controller,
                        reader,
                        err.clone(),
                    )?;

                    return Err(ctx.throw(err));
                },
                // Let chunkSize be result.[[Value]].
                Ok(chunk_size) => {
                    // Let enqueueResult be EnqueueValueWithSize(controller, chunk, chunkSize).
                    let enqueue_result =
                        controller.enqueue_value_with_size(&ctx, chunk, chunk_size);

                    match enqueue_result {
                        // If enqueueResult is an abrupt completion,
                        Err(Error::Exception) => {
                            let err = ctx.catch();
                            // Perform ! ReadableStreamDefaultControllerError(controller, enqueueResult.[[Value]]).
                            Self::readable_stream_default_controller_error(
                                &ctx,
                                stream,
                                controller,
                                reader,
                                err.clone(),
                            )?;
                            return Err(ctx.throw(err));
                        },
                        Err(err) => return Err(err),
                        Ok(()) => {},
                    }
                },
                Err(err) => return Err(err),
            }
        }

        // Perform ! ReadableStreamDefaultControllerCallPullIfNeeded(controller).
        Self::readable_stream_default_controller_call_pull_if_needed(
            ctx, controller, stream, reader,
        )?;

        Ok(())
    }

    fn enqueue_value_with_size(
        &mut self,
        ctx: &Ctx<'js>,
        value: Value<'js>,
        size: Value<'js>,
    ) -> Result<()> {
        let size = match is_non_negative_number(size) {
            None => {
                // If ! IsNonNegativeNumber(size) is false, throw a RangeError exception.
                return Err(Exception::throw_range(
                    ctx,
                    "Size must be a finite, non-NaN, non-negative number.",
                ));
            },
            Some(size) => size,
        };

        // If size is +∞, throw a RangeError exception.
        if size.is_infinite() {
            return Err(Exception::throw_range(
                ctx,
                "Size must be a finite, non-NaN, non-negative number.",
            ));
        };

        // Append a new value-with-size with value value and size size to container.[[queue]].
        self.queue.push_back(ValueWithSize { value, size });

        // Set container.[[queueTotalSize]] to container.[[queueTotalSize]] + size.
        self.queue_total_size += size;

        Ok(())
    }

    pub(super) fn pull_steps(
        ctx: &Ctx<'js>,
        mut controller: OwnedBorrowMut<'js, Self>,
        // Let stream be this.[[stream]].
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        // If this.[[queue]] is not empty,
        if !controller.queue.is_empty() {
            // Let chunk be ! DequeueValue(this).
            let chunk = dequeue_value(&mut controller);
            // If this.[[closeRequested]] is true and this.[[queue]] is empty,
            if controller.close_requested && controller.queue.is_empty() {
                // Perform ! ReadableStreamDefaultControllerClearAlgorithms(this).
                controller.readable_stream_default_controller_clear_algorithms();
                let mut c = controller.into();
                // Perform ! ReadableStreamClose(stream).
                (stream, c, reader) =
                    ReadableStream::readable_stream_close(ctx.clone(), stream, c, reader)?;
                controller = c
                    .into_default_controller()
                    .expect("readable_stream_close must return the same type of controller")
            } else {
                // Otherwise, perform ! ReadableStreamDefaultControllerCallPullIfNeeded(this).
                (stream, controller, reader) =
                    Self::readable_stream_default_controller_call_pull_if_needed(
                        ctx.clone(),
                        controller,
                        stream,
                        reader,
                    )?;
            }

            // Perform readRequest’s chunk steps, given chunk.
            read_request.chunk_steps(stream, controller.into(), reader, chunk)?;
        } else {
            // Otherwise,
            // Perform ! ReadableStreamAddReadRequest(stream, readRequest).
            stream.readable_stream_add_read_request(reader.as_mut(), read_request);
            // Perform ! ReadableStreamDefaultControllerCallPullIfNeeded(this).

            Self::readable_stream_default_controller_call_pull_if_needed(
                ctx.clone(),
                controller,
                stream,
                reader,
            )?;
        }

        Ok(())
    }

    pub(super) fn cancel_steps(
        ctx: &Ctx<'js>,
        mut controller: OwnedBorrowMut<'js, Self>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        reason: Value<'js>,
    ) -> Result<(
        Promise<'js>,
        Class<'js, ReadableStream<'js>>,
        OwnedBorrowMut<'js, Self>,
        Option<ReadableStreamReader<'js>>,
    )> {
        // Perform ! ResetQueue(this).
        controller.reset_queue();

        // Let result be the result of performing this.[[cancelAlgorithm]], passing reason.
        let (result, controller, stream, reader) =
            Self::cancel_algorithm(ctx.clone(), controller, stream, reader, reason)?;

        let mut controller = OwnedBorrowMut::from_class(controller);
        // Perform ! ReadableStreamDefaultControllerClearAlgorithms(this).
        controller.readable_stream_default_controller_clear_algorithms();

        // Return result.
        Ok((result, stream, controller, reader))
    }

    pub(super) fn release_steps(&mut self) {}

    fn start_algorithm(
        ctx: Ctx<'js>,
        controller: OwnedBorrowMut<'js, Self>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        start_algorithm: StartAlgorithm<'js>,
    ) -> Result<(
        Value<'js>,
        Class<'js, Self>,
        Class<'js, ReadableStream<'js>>,
    )> {
        let controller_class = controller.into_inner();
        let stream_class = stream.into_inner();

        Ok((
            start_algorithm.call(
                ctx,
                ReadableStreamController::ReadableStreamDefaultController(controller_class.clone()),
            )?,
            controller_class,
            stream_class,
        ))
    }

    fn pull_algorithm(
        ctx: Ctx<'js>,
        controller: OwnedBorrowMut<'js, Self>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
    ) -> Result<(
        Promise<'js>,
        Class<'js, Self>,
        Class<'js, ReadableStream<'js>>,
        Option<ReadableStreamReader<'js>>,
    )> {
        let pull_algorithm = controller
            .pull_algorithm
            .clone()
            .expect("pull algorithm used after ReadableStreamDefaultControllerClearAlgorithms");
        let controller_class = controller.into_inner();
        let stream_class = stream.into_inner();
        let reader_class = reader.map(|r| r.into_inner());

        Ok((
            pull_algorithm.call(
                ctx,
                ReadableStreamController::ReadableStreamDefaultController(controller_class.clone()),
            )?,
            controller_class,
            stream_class,
            reader_class,
        ))
    }

    fn strategy_size_algorithm(
        ctx: Ctx<'js>,
        controller: OwnedBorrowMut<'js, Self>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        chunk: Value<'js>,
    ) -> (
        Result<Value<'js>>,
        Class<'js, Self>,
        Class<'js, ReadableStream<'js>>,
        Option<ReadableStreamReader<'js>>,
    ) {
        let strategy_size_algorithm = controller
            .strategy_size_algorithm
            .clone()
            .expect("size algorithm used after ReadableStreamDefaultControllerClearAlgorithms");
        let controller_class = controller.into_inner();
        let stream_class = stream.into_inner();
        let reader_class = reader.map(|r| r.into_inner());

        (
            strategy_size_algorithm.call(ctx, chunk),
            controller_class,
            stream_class,
            reader_class,
        )
    }

    fn cancel_algorithm(
        ctx: Ctx<'js>,
        controller: OwnedBorrowMut<'js, Self>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        reader: Option<ReadableStreamReaderOwnedBorrowMut<'js>>,
        reason: Value<'js>,
    ) -> Result<(
        Promise<'js>,
        Class<'js, Self>,
        Class<'js, ReadableStream<'js>>,
        Option<ReadableStreamReader<'js>>,
    )> {
        let cancel_algorithm = controller
            .cancel_algorithm
            .clone()
            .expect("cancel algorithm used after ReadableStreamDefaultControllerClearAlgorithms");
        let controller_class = controller.into_inner();
        let stream_class = stream.into_inner();
        let reader_class = reader.map(|r| r.into_inner());

        Ok((
            cancel_algorithm.call(ctx, reason)?,
            controller_class,
            stream_class,
            reader_class,
        ))
    }
}

#[methods(rename_all = "camelCase")]
impl<'js> ReadableStreamDefaultController<'js> {
    // this is required by web platform tests for unclear reasons
    fn constructor() -> Self {
        unimplemented!()
    }

    #[qjs(constructor)]
    fn new(ctx: Ctx<'js>) -> Result<Class<'js, Self>> {
        Err(Exception::throw_type(&ctx, "Illegal constructor"))
    }

    // readonly attribute unrestricted double? desiredSize;
    #[qjs(get)]
    fn desired_size(&self) -> Null<f64> {
        let stream = OwnedBorrow::from_class(
            self.stream
                .clone()
                .expect("desiredSize() called without stream"),
        );
        self.readable_stream_default_controller_get_desired_size(&stream)
    }

    // undefined close();
    fn close(ctx: Ctx<'js>, controller: This<OwnedBorrowMut<'js, Self>>) -> Result<()> {
        let stream = OwnedBorrowMut::from_class(
            controller
                .stream
                .clone()
                .expect("close() called on ReadableStreamDefaultController without stream"),
        );

        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(this) is false, throw a TypeError exception.
        if !controller.readable_stream_default_controller_can_close_or_enqueue(&stream) {
            return Err(Exception::throw_type(
                &ctx,
                "The stream is not in a state that permits close",
            ));
        }

        // Perform ! ReadableStreamDefaultControllerClose(this).
        Self::readable_stream_default_controller_close(ctx, stream, controller.0)
    }

    // undefined enqueue(optional any chunk);
    fn enqueue(
        ctx: Ctx<'js>,
        controller: This<OwnedBorrowMut<'js, Self>>,
        chunk: Opt<Value<'js>>,
    ) -> Result<()> {
        let mut stream = OwnedBorrowMut::from_class(
            controller
                .stream
                .clone()
                .expect("enqueue() called on ReadableStreamDefaultController without stream"),
        );

        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(this) is false, throw a TypeError exception.
        if !controller
            .0
            .readable_stream_default_controller_can_close_or_enqueue(&stream)
        {
            return Err(Exception::throw_type(
                &ctx,
                "The stream is not in a state that permits enqueue",
            ));
        }

        let reader = stream.reader_mut();

        // Perform ? ReadableStreamDefaultControllerEnqueue(this, chunk).
        Self::readable_stream_default_controller_enqueue(
            ctx.clone(),
            controller.0,
            stream,
            reader,
            chunk.0.unwrap_or(Value::new_undefined(ctx)),
        )
    }

    // undefined error(optional any e);
    fn error(
        ctx: Ctx<'js>,
        controller: This<OwnedBorrowMut<'js, Self>>,
        e: Opt<Value<'js>>,
    ) -> Result<()> {
        let mut stream = OwnedBorrowMut::from_class(
            controller
                .stream
                .clone()
                .expect("error() called on ReadableStreamDefaultController without stream"),
        );
        let reader = stream.reader_mut();
        // Perform ! ReadableStreamDefaultControllerError(this, e).
        Self::readable_stream_default_controller_error(
            &ctx,
            stream,
            controller.0,
            reader,
            e.0.unwrap_or(Value::new_undefined(ctx.clone())),
        )
    }
}

fn is_non_negative_number(value: Value<'_>) -> Option<f64> {
    // If Type(v) is not Number, return false.
    let number = value.as_number()?;
    // If v is NaN, return false.
    if number.is_nan() {
        return None;
    }

    // If v < 0, return false.
    if number < 0.0 {
        return None;
    }

    // Return true.
    Some(number)
}

fn dequeue_value<'js>(container: &mut ReadableStreamDefaultController<'js>) -> Value<'js> {
    // Let valueWithSize be container.[[queue]][0].
    // Remove valueWithSize from container.[[queue]].
    let value_with_size = container
        .queue
        .pop_front()
        .expect("DequeueValue called with empty queue");
    // Set container.[[queueTotalSize]] to container.[[queueTotalSize]] − valueWithSize’s size.
    container.queue_total_size -= value_with_size.size;
    // If container.[[queueTotalSize]] < 0, set container.[[queueTotalSize]] to 0. (This can occur due to rounding errors.)
    if container.queue_total_size < 0.0 {
        container.queue_total_size = 0.0
    }
    value_with_size.value
}
