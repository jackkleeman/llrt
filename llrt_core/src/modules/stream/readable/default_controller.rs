use std::{borrow::Borrow, collections::VecDeque};

use rquickjs::{
    class::Trace, methods, prelude::This, Class, Ctx, Error, Exception, Function, Result, Value,
};

use crate::modules::stream::readable::{CancelAlgorithm, PullAlgorithm, StartAlgorithm};

use super::{
    promise_resolved_with, upon_promise, ReadableStream, ReadableStreamController,
    ReadableStreamReadRequest, ReadableStreamState, SizeAlgorithm, UnderlyingSource,
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
        stream: Class<'js, ReadableStream<'js>>,
        underlying_source: Option<UnderlyingSource<'js>>,
        high_water_mark: f64,
        size_algorithm: SizeAlgorithm<'js>,
    ) -> Result<()> {
        // Let controller be a new ReadableStreamDefaultController.
        let controller = Self::default();

        let (start_algorithm, pull_algorithm, cancel_algorithm) =
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
                            underlying_source: underlying_source.js.clone(),
                        })
                        .unwrap_or(PullAlgorithm::ReturnPromiseUndefined),
                    // If underlyingSourceDict["cancel"] exists, then set cancelAlgorithm to an algorithm which takes an argument reason and returns the result of invoking underlyingSourceDict["cancel"] with argument list
                    // « reason » and callback this value underlyingSource.
                    underlying_source
                        .cancel
                        .map(|f| CancelAlgorithm::Function {
                            f,
                            underlying_source: underlying_source.js,
                        })
                        .unwrap_or(CancelAlgorithm::ReturnPromiseUndefined),
                )
            } else {
                (
                    StartAlgorithm::ReturnUndefined,
                    PullAlgorithm::ReturnPromiseUndefined,
                    CancelAlgorithm::ReturnPromiseUndefined,
                )
            };

        // Perform ? SetUpReadableStreamDefaultController(stream, controller, startAlgorithm, pullAlgorithm, cancelAlgorithm, highWaterMark, sizeAlgorithm).
        Self::set_up_readable_stream_default_controller(
            ctx.clone(),
            stream,
            Class::instance(ctx.clone(), controller)?,
            start_algorithm,
            pull_algorithm,
            cancel_algorithm,
            high_water_mark,
            size_algorithm,
        )
    }

    fn set_up_readable_stream_default_controller(
        ctx: Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
        controller: Class<'js, Self>,
        start_algorithm: StartAlgorithm<'js>,
        pull_algorithm: PullAlgorithm<'js>,
        cancel_algorithm: CancelAlgorithm<'js>,
        high_water_mark: f64,
        size_algorithm: SizeAlgorithm<'js>,
    ) -> Result<()> {
        {
            let mut controller = controller.borrow_mut();

            // Set controller.[[stream]] to stream.
            controller.stream = Some(stream.clone());

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
        }

        // Set stream.[[controller]] to controller.
        stream.borrow_mut().controller = Some(
            ReadableStreamController::ReadableStreamDefaultController(controller.clone()),
        );

        // Let startResult be the result of performing startAlgorithm. (This might throw an exception.)
        let start_result: Result<Value> = match start_algorithm {
            StartAlgorithm::ReturnUndefined => Ok(Value::new_undefined(ctx.clone())),
            StartAlgorithm::Function {
                f,
                underlying_source,
            } => f.call((This(underlying_source), controller.clone())),
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
                    Self::readable_stream_default_controller_call_pull_if_needed(
                        controller.clone(),
                        ctx.clone(),
                        stream.clone(),
                    )
                }
            })?,
            // Upon rejection of startPromise with reason r,
            Function::new(ctx.clone(), {
                let controller = controller.clone();
                move |r: Value<'js>| {
                    // Perform ! ReadableByteStreamControllerError(controller, r).
                    Self::readable_stream_default_controller_error(controller.clone(), r)
                }
            })?,
        )?;

        Ok(())
    }

    fn reset_queue(&mut self) {
        // Set container.[[queue]] to a new empty list.
        self.queue.clear();
        // Set container.[[queueTotalSize]] to 0.
        self.queue_total_size = 0.0;
    }

    fn readable_stream_default_controller_call_pull_if_needed(
        controller: Class<'js, Self>,
        ctx: Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
    ) -> Result<()> {
        {
            let mut controller = controller.borrow_mut();
            // Let shouldPull be ! ReadableStreamDefaultControllerShouldCallPull(controller).
            let should_pull =
                controller.readable_stream_default_controller_should_call_pull(&stream);

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
                panic!("pull algorithm used after ReadableStreamDefaultControllerClearAlgorithms")
            },
            Some(PullAlgorithm::ReturnPromiseUndefined) => {
                promise_resolved_with(&ctx, Ok(Value::new_undefined(ctx.clone())))?
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
                // Upon fulfillment of pullPromise,
                move || {
                    let mut controller_mut = controller.borrow_mut();
                    // Set controller.[[pulling]] to false.
                    controller_mut.pulling = false;
                    // If controller.[[pullAgain]] is true,
                    if controller_mut.pull_again {
                        // Set controller.[[pullAgain]] to false.
                        controller_mut.pull_again = false;
                        drop(controller_mut);
                        // Perform ! ReadableStreamDefaultControllerCallPullIfNeeded(controller).
                        Self::readable_stream_default_controller_call_pull_if_needed(
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
                // Upon rejection of pullPromise with reason e,
                move |e: Value<'js>| {
                    // Perform ! ReadableStreamDefaultControllerError(controller, e).
                    Self::readable_stream_default_controller_error(controller.clone(), e)
                }
            })?,
        )?;

        Ok(())
    }

    fn readable_stream_default_controller_error(
        controller: Class<'js, Self>,
        e: Value<'js>,
    ) -> Result<()> {
        // Let stream be controller.[[stream]].
        let stream = controller.borrow().stream.clone();
        let stream = match stream {
            Some(stream) if stream.borrow().state == ReadableStreamState::Readable => stream,
            // If stream.[[state]] is not "readable", return.
            _ => return Ok(()),
        };

        {
            let mut controller = controller.borrow_mut();

            // Perform ! ResetQueue(controller).
            controller.reset_queue();

            // Perform ! ReadableStreamDefaultControllerClearAlgorithms(controller).
            controller.readable_stream_default_controller_clear_algorithms();
        }

        // Perform ! ReadableStreamError(stream, e).
        stream.borrow_mut().readable_stream_error(e)?;

        Ok(())
    }

    fn readable_stream_default_controller_should_call_pull(
        &self,
        stream: &Class<'js, ReadableStream<'js>>,
    ) -> bool {
        // Let stream be controller.[[stream]].
        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(controller) is false, return false.
        if !self.readable_stream_default_controller_can_close_or_enqueue() {
            return false;
        }

        // If controller.[[started]] is false, return false.
        if !self.started {
            return false;
        }

        {
            let stream = stream.borrow();
            // If ! IsReadableStreamLocked(stream) is true and ! ReadableStreamGetNumReadRequests(stream) > 0, return true.
            if stream.is_readable_stream_locked()
                && stream.readable_stream_get_num_read_requests() > 0
            {
                return true;
            }
        }

        // Let desiredSize be ! ReadableStreamDefaultControllerGetDesiredSize(controller).
        let desired_size = self
            .readable_stream_default_controller_get_desired_size()
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

    fn readable_stream_default_controller_can_close_or_enqueue(&self) -> bool {
        // Let state be controller.[[stream]].[[state]].
        // If controller.[[closeRequested]] is false and state is "readable", return true.
        if !self.close_requested {
            if let Some(stream) = &self.stream {
                if let ReadableStreamState::Readable = stream.borrow().state {
                    return true;
                }
            }
        }
        // Otherwise, return false.
        false
    }

    fn readable_stream_default_controller_get_desired_size(&self) -> Option<f64> {
        let stream = self
            .stream
            .clone()
            .expect("ReadableStreamDefaultControllerGetDesiredSize called without stream");
        let stream = stream.borrow();
        // Let state be controller.[[stream]].[[state]].
        match stream.borrow().state {
            // If state is "errored", return null.
            ReadableStreamState::Errored => None,
            // If state is "closed", return 0.
            ReadableStreamState::Closed => Some(0.0),
            // Return controller.[[strategyHWM]] − controller.[[queueTotalSize]].
            ReadableStreamState::Readable => Some(self.strategy_hwm - self.queue_total_size),
        }
    }

    fn readable_stream_default_controller_close(&mut self) -> Result<()> {
        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(controller) is false, return.
        if !self.readable_stream_default_controller_can_close_or_enqueue() {
            return Ok(());
        }

        // Let stream be controller.[[stream]].
        let stream = self.stream.clone();

        // Set controller.[[closeRequested]] to true.
        self.close_requested = true;

        // If controller.[[queue]] is empty,
        if self.queue.is_empty() {
            // Perform ! ReadableStreamDefaultControllerClearAlgorithms(controller).
            self.readable_stream_default_controller_clear_algorithms();
            // Perform ! ReadableStreamClose(stream).
            ReadableStream::readable_stream_close(
                stream.expect("ReadableStreamDefaultControllerClose called without stream"),
            )?;
        }

        Ok(())
    }

    fn readable_stream_default_controller_enqueue(
        controller: Class<'js, Self>,
        ctx: Ctx<'js>,
        chunk: Value<'js>,
    ) -> Result<()> {
        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(controller) is false, return.
        if !controller
            .borrow()
            .readable_stream_default_controller_can_close_or_enqueue()
        {
            return Ok(());
        }

        // Let stream be controller.[[stream]].
        let stream = controller
            .borrow()
            .stream
            .clone()
            .expect("ReadableStreamDefaultControllerEnqueue called without stream");
        {
            let stream = stream.borrow();

            // If ! IsReadableStreamLocked(stream) is true and ! ReadableStreamGetNumReadRequests(stream) > 0, perform ! ReadableStreamFulfillReadRequest(stream, chunk, false).
            if stream.borrow().is_readable_stream_locked()
                && stream.readable_stream_get_num_read_requests() > 0
            {
                stream.readable_stream_fulfill_read_request(chunk, false)?;
            } else {
                // Let result be the result of performing controller.[[strategySizeAlgorithm]], passing in chunk, and interpreting the result as a completion record.
                let result = match controller.borrow().strategy_size_algorithm {
                    None => {
                        panic!(
                        "size algorithm used after ReadableStreamDefaultControllerClearAlgorithms"
                    )
                    },
                    Some(SizeAlgorithm::AlwaysOne) => Ok(Value::new_number(ctx.clone(), 1.0)),
                    Some(SizeAlgorithm::SizeFunction(ref f)) => f.call((chunk.clone(),))?,
                };

                match result {
                    // If result is an abrupt completion,
                    Err(err @ Error::Exception) => {
                        // Perform ! ReadableStreamDefaultControllerError(controller, result.[[Value]]).
                        Self::readable_stream_default_controller_error(controller, ctx.catch())?;
                        return Err(err);
                    },
                    // Let chunkSize be result.[[Value]].
                    Ok(chunk_size) => {
                        // Let enqueueResult be EnqueueValueWithSize(controller, chunk, chunkSize).
                        let enqueue_result = controller
                            .borrow_mut()
                            .enqueue_value_with_size(&ctx, chunk, chunk_size);

                        match enqueue_result {
                            // If enqueueResult is an abrupt completion,
                            Err(err @ Error::Exception) => {
                                // Perform ! ReadableStreamDefaultControllerError(controller, enqueueResult.[[Value]]).
                                Self::readable_stream_default_controller_error(
                                    controller,
                                    ctx.catch(),
                                )?;
                                return Err(err);
                            },
                            Err(err) => return Err(err),
                            Ok(()) => {},
                        }
                    },
                    Err(err) => return Err(err),
                }
            }
        }

        // Perform ! ReadableStreamDefaultControllerCallPullIfNeeded(controller).
        Self::readable_stream_default_controller_call_pull_if_needed(controller, ctx, stream)?;

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
        controller: Class<'js, Self>,
        ctx: &Ctx<'js>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        // Let stream be this.[[stream]].
        let stream = controller
            .borrow()
            .stream
            .clone()
            .expect("ReadableStreamDefaultController pullSteps called without stream");

        // If this.[[queue]] is not empty,
        if !controller.borrow().queue.is_empty() {
            // Let chunk be ! DequeueValue(this).
            let chunk = dequeue_value(&mut controller.borrow_mut());
            {
                let mut controller_mut = controller.borrow_mut();
                // If this.[[closeRequested]] is true and this.[[queue]] is empty,
                if controller_mut.close_requested && controller_mut.queue.is_empty() {
                    // Perform ! ReadableStreamDefaultControllerClearAlgorithms(this).
                    controller_mut.readable_stream_default_controller_clear_algorithms();
                    controller_mut.readable_stream_default_controller_close()?;
                } else {
                    drop(controller_mut);
                    // Otherwise, perform ! ReadableStreamDefaultControllerCallPullIfNeeded(this).
                    ReadableStreamDefaultController::readable_stream_default_controller_call_pull_if_needed(controller, ctx.clone(), stream)?;
                }

                // Perform readRequest’s chunk steps, given chunk.
                read_request.chunk_steps.call((chunk,))?;
            }
        } else {
            // Otherwise,
            // Perform ! ReadableStreamAddReadRequest(stream, readRequest).
            stream
                .borrow()
                .readable_stream_add_read_request(read_request);
            // Perform ! ReadableStreamDefaultControllerCallPullIfNeeded(this).
            Self::readable_stream_default_controller_call_pull_if_needed(
                controller,
                ctx.clone(),
                stream,
            )?
        }

        Ok(())
    }
}

#[methods]
impl<'js> ReadableStreamDefaultController<'js> {
    // readonly attribute unrestricted double? desiredSize;
    #[qjs(get)]
    fn desired_size(controller: This<Class<'js, Self>>) -> Option<f64> {
        let controller = controller.0.borrow();
        controller.readable_stream_default_controller_get_desired_size()
    }

    // undefined close();
    fn close(controller: This<Class<'js, Self>>, ctx: Ctx<'js>) -> Result<()> {
        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(this) is false, throw a TypeError exception.
        if !controller
            .0
            .borrow()
            .readable_stream_default_controller_can_close_or_enqueue()
        {
            return Err(Exception::throw_type(
                &ctx,
                "The stream is not in a state that permits close",
            ));
        }

        // Perform ! ReadableStreamDefaultControllerClose(this).
        controller
            .0
            .borrow_mut()
            .readable_stream_default_controller_close()
    }

    // undefined enqueue(optional any chunk);
    fn enqueue(controller: This<Class<'js, Self>>, ctx: Ctx<'js>, chunk: Value<'js>) -> Result<()> {
        // If ! ReadableStreamDefaultControllerCanCloseOrEnqueue(this) is false, throw a TypeError exception.
        if !controller
            .0
            .borrow()
            .readable_stream_default_controller_can_close_or_enqueue()
        {
            return Err(Exception::throw_type(
                &ctx,
                "The stream is not in a state that permits enqueue",
            ));
        }

        // Perform ? ReadableStreamDefaultControllerEnqueue(this, chunk).
        Self::readable_stream_default_controller_enqueue(controller.0, ctx, chunk)
    }

    // undefined error(optional any e);
    fn error(controller: This<Class<'js, Self>>, e: Value<'js>) -> Result<()> {
        // Perform ! ReadableStreamDefaultControllerError(this, e).
        Self::readable_stream_default_controller_error(controller.0, e)
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
