use std::collections::VecDeque;

use llrt_utils::{
    bytes::ObjectBytes,
    error_messages::{ERROR_MSG_NOT_ARRAY_BUFFER, ERROR_MSG_NOT_ARRAY_BUFFER_VIEW},
    result::ResultExt,
};
use rquickjs::{
    atom::PredefinedAtom, class::Trace, function::Constructor, prelude::This, ArrayBuffer, Class,
    Ctx, Error, Exception, IntoJs, Object, Promise, Result, TypedArray, Value,
};

use super::{
    byob_reader::ReadableStreamReadIntoRequest, copy_data_block_bytes, promise_resolved_with,
    transfer_array_buffer, upon_promise, CancelAlgorithm, PullAlgorithm, ReadableStream,
    ReadableStreamController, ReadableStreamReadRequest, ReadableStreamReader, ReadableStreamState,
    StartAlgorithm, UnderlyingSource,
};

#[derive(Trace, Default)]
#[rquickjs::class]
pub(super) struct ReadableStreamByteController<'js> {
    auto_allocate_chunk_size: Option<usize>,
    #[qjs(get)]
    byob_request: Option<ReadableStreamBYOBRequest<'js>>,
    cancel_algorithm: Option<CancelAlgorithm<'js>>,
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
    pub(super) fn set_up_readable_byte_stream_controller_from_underlying_source(
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
                    // Let autoAllocateChunkSize be underlyingSourceDict["autoAllocateChunkSize"], if it exists, or undefined otherwise.
                    underlying_source.auto_allocate_chunk_size,
                )
            } else {
                (
                    StartAlgorithm::ReturnUndefined,
                    PullAlgorithm::ReturnPromiseUndefined,
                    CancelAlgorithm::ReturnPromiseUndefined,
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
        cancel_algorithm: CancelAlgorithm<'js>,
        high_water_mark: usize,
        auto_allocate_chunk_size: Option<usize>,
    ) -> Result<()> {
        {
            let mut controller = controller.borrow_mut();

            // Set controller.[[stream]] to stream.
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
            controller.cancel_algorithm = Some(cancel_algorithm);

            // Set controller.[[autoAllocateChunkSize]] to autoAllocateChunkSize.
            controller.auto_allocate_chunk_size = auto_allocate_chunk_size;

            // Set controller.[[pendingPullIntos]] to a new empty list.
            controller.pending_pull_intos.clear();
        }

        stream.borrow_mut().controller = Some(
            ReadableStreamController::ReadableStreamByteController(controller.clone()),
        );

        // Let startResult be the result of performing startAlgorithm.
        let start_result: Result<Value> = match start_algorithm {
            StartAlgorithm::ReturnUndefined => Ok(Value::new_undefined(ctx.clone())),
            StartAlgorithm::Function {
                f,
                underlying_source,
            } => f.call((This(underlying_source), controller.clone())),
        };

        // Let startPromise be a promise resolved with startResult.
        let start_promise = promise_resolved_with(&ctx, start_result)?;

        let _ = upon_promise::<Value<'js>, _>(ctx.clone(), start_promise, {
            let ctx = ctx.clone();
            let stream = stream.clone();
            move |result| match result {
                // Upon fulfillment of startPromise,
                Ok(_) => {
                    let mut controller_mut = controller.borrow_mut();
                    // Set controller.[[started]] to true.
                    controller_mut.started = true;
                    // Perform ! ReadableByteStreamControllerCallPullIfNeeded(controller).
                    controller_mut.readable_byte_stream_controller_call_pull_if_needed(
                        ctx.clone(),
                        controller.clone(),
                        stream.clone(),
                    )?;
                    Ok(())
                },
                // Upon rejection of startPromise with reason r,
                Err(r) => {
                    // Perform ! ReadableByteStreamControllerError(controller, r).
                    controller
                        .borrow_mut()
                        .readable_byte_stream_controller_error(&ctx, r)
                },
            }
        })?;

        Ok(())
    }

    fn readable_byte_stream_controller_call_pull_if_needed(
        &mut self,
        ctx: Ctx<'js>,
        controller: Class<'js, Self>,
        stream: Class<'js, ReadableStream<'js>>,
    ) -> Result<()> {
        // Let shouldPull be ! ReadableByteStreamControllerShouldCallPull(controller).
        let should_pull = self.readable_byte_stream_controller_should_call_pull(&stream);

        // If shouldPull is false, return.
        if !should_pull {
            return Ok(());
        }

        // If controller.[[pulling]] is true,
        if self.pulling {
            // Set controller.[[pullAgain]] to true.
            self.pull_again = true;

            // Return.
            return Ok(());
        }

        // Set controller.[[pulling]] to true.
        self.pulling = true;

        // Let pullPromise be the result of performing controller.[[pullAlgorithm]].
        let pull_promise = match self.pull_algorithm {
            None => {
                panic!("pull algorithm used after ReadableByteStreamControllerClearAlgorithms")
            },
            Some(PullAlgorithm::ReturnPromiseUndefined) => {
                promise_resolved_with(&ctx, Ok(Value::new_undefined(ctx.clone())))?
            },
            Some(PullAlgorithm::Function {
                ref f,
                ref underlying_source,
            }) => {
                let f = f.clone();
                let underying_source = underlying_source.clone();
                f.call((This(underying_source), controller.clone()))?
            },
        };

        upon_promise::<Value<'js>, _>(ctx.clone(), pull_promise, move |result| {
            let mut controller_mut = controller.borrow_mut();
            match result {
                Ok(_) => {
                    controller_mut.pulling = false;
                    if controller_mut.pull_again {
                        controller_mut.pull_again = false;
                        controller_mut.readable_byte_stream_controller_call_pull_if_needed(
                            ctx.clone(),
                            controller.clone(),
                            stream.clone(),
                        )?;
                    };
                    Ok(())
                },
                Err(e) => controller_mut.readable_byte_stream_controller_error(&ctx, e),
            }
        })?;

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

    fn readable_byte_stream_controller_error(
        &mut self,
        ctx: &Ctx<'js>,
        e: Value<'js>,
    ) -> Result<()> {
        // Let stream be controller.[[stream]].
        let stream = self.stream.clone();
        let stream = match stream {
            Some(stream) if stream.borrow().state == ReadableStreamState::Readable => stream,
            // If stream.[[state]] is not "readable", return.
            _ => return Ok(()),
        };

        // Perform ! ReadableByteStreamControllerClearPendingPullIntos(controller).
        self.readable_byte_stream_controller_clear_pending_pull_intos();

        // Perform ! ResetQueue(controller).
        self.reset_queue();

        // Perform ! ReadableByteStreamControllerClearAlgorithms(controller).
        self.readable_byte_stream_controller_clear_algorithms();

        // Perform ! ReadableStreamError(stream, e).
        stream.borrow_mut().readable_stream_error(ctx, e)?;

        Ok(())
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

    fn readable_byte_stream_controller_close(&mut self, ctx: &Ctx<'js>) -> Result<()> {
        // Let stream be controller.[[stream]].
        // If controller.[[closeRequested]] is true or stream.[[state]] is not "readable", return.
        if self.close_requested
            || self.stream.as_ref().map(|s| s.borrow().state) != Some(ReadableStreamState::Readable)
        {
            return Ok(());
        }

        // If controller.[[queueTotalSize]] > 0,
        if self.queue_total_size > 0 {
            // Set controller.[[closeRequested]] to true.
            self.close_requested = true;
            // Return.
            return Ok(());
        }

        // If controller.[[pendingPullIntos]] is not empty,
        // Let firstPendingPullInto be controller.[[pendingPullIntos]][0].
        if let Some(first_pending_pull_into) = self.pending_pull_intos.front() {
            // If the remainder after dividing firstPendingPullInto’s bytes filled by firstPendingPullInto’s element size is not 0,
            if first_pending_pull_into.bytes_filled % first_pending_pull_into.element_size != 0 {
                // Let e be a new TypeError exception.
                let e: Value = ctx.eval(
                    r#"new TypeError("Insufficient bytes to fill elements in the given buffer")"#,
                )?;
                self.readable_byte_stream_controller_error(ctx, e.clone())?;
                return Err(ctx.throw(e));
            }
        }

        Ok(())
    }

    fn readable_byte_stream_controller_enqueue(
        &mut self,
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
        chunk: ObjectBytes<'js>,
    ) -> Result<()> {
        // Let stream be controller.[[stream]].
        let stream = self.stream.as_ref();

        // If controller.[[closeRequested]] is true or stream.[[state]] is not "readable", return.
        if self.close_requested
            || stream.map(|s| s.borrow().state) != Some(ReadableStreamState::Readable)
        {
            return Ok(());
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

        // If controller.[[pendingPullIntos]] is not empty,
        // Let firstPendingPullInto be controller.[[pendingPullIntos]][0].
        if !self.pending_pull_intos.is_empty() {
            // If ! IsDetachedBuffer(firstPendingPullInto’s buffer) is true, throw a TypeError exception.
            self.pending_pull_intos[0]
                    .buffer
                    .as_raw()
                    .ok_or(Exception::throw_type(
                        ctx,
                        "The BYOB request's buffer has been detached and so cannot be filled with an enqueued chunk",
                    ))?;

            let existing_buffer = self.pending_pull_intos[0].buffer.clone();

            // Perform ! ReadableByteStreamControllerInvalidateBYOBRequest(controller).
            self.readable_byte_stream_controller_invalidate_byob_request();

            // Set firstPendingPullInto’s buffer to ! TransferArrayBuffer(firstPendingPullInto’s buffer).
            self.pending_pull_intos[0].buffer =
                transfer_array_buffer(ctx.clone(), existing_buffer)?;

            // If firstPendingPullInto’s reader type is "none", perform ? ReadableByteStreamControllerEnqueueDetachedPullIntoToQueue(controller, firstPendingPullInto).
            if let PullIntoDescriptorReaderType::None = self.pending_pull_intos[0].reader_type {
                self.readable_byte_stream_enqueue_detached_pull_into_to_queue(ctx.clone(), 0)?;
            }
        }

        let stream = self.stream.clone().unwrap();

        if stream.borrow().readable_stream_has_default_reader() {
            // If ! ReadableStreamHasDefaultReader(stream) is true,
            // Perform ! ReadableByteStreamControllerProcessReadRequestsUsingQueue(controller).
            self.readable_byte_stream_controller_process_read_requests_using_queue(
                ctx,
                controller.clone(),
            )?;

            // If ! ReadableStreamGetNumReadRequests(stream) is 0,
            if stream.borrow().readable_stream_get_num_read_requests() == 0 {
                // Perform ! ReadableByteStreamControllerEnqueueChunkToQueue(controller, transferredBuffer, byteOffset, byteLength).
                self.readable_byte_stream_controller_enqueue_chunk_to_queue(
                    transferred_buffer,
                    byte_offset,
                    byte_length,
                )
            } else {
                // Otherwise,
                // If controller.[[pendingPullIntos]] is not empty,
                if !self.pending_pull_intos.is_empty() {
                    // Perform ! ReadableByteStreamControllerShiftPendingPullInto(controller).
                    self.readable_byte_stream_controller_shift_pending_pull_into();
                }

                // Let transferredView be ! Construct(%Uint8Array%, « transferredBuffer, byteOffset, byteLength »).
                let ctor: Constructor = ctx.globals().get(PredefinedAtom::Uint8Array)?;
                let transferred_view: ObjectBytes =
                    ctor.construct((transferred_buffer, byte_offset, byte_length))?;

                // Perform ! ReadableStreamFulfillReadRequest(stream, transferredView, false).
                ReadableStream::readable_stream_fulfill_read_request(
                    &stream.borrow(),
                    transferred_view.into_js(ctx)?,
                    false,
                )?
            }
        } else if stream.borrow().readable_stream_has_byob_reader() {
            // Otherwise, if ! ReadableStreamHasBYOBReader(stream) is true,
            // Perform ! ReadableByteStreamControllerEnqueueChunkToQueue(controller, transferredBuffer, byteOffset, byteLength).
            self.readable_byte_stream_controller_enqueue_chunk_to_queue(
                transferred_buffer,
                byte_offset,
                byte_length,
            );
            // Perform ! ReadableByteStreamControllerProcessPullIntoDescriptorsUsingQueue(controller).
            self.readable_byte_stream_controller_process_pull_into_descriptors_using_queue(ctx)?;
        } else {
            // Otherwise,
            // Perform ! ReadableByteStreamControllerEnqueueChunkToQueue(controller, transferredBuffer, byteOffset, byteLength).
            self.readable_byte_stream_controller_enqueue_chunk_to_queue(
                transferred_buffer,
                byte_offset,
                byte_length,
            );
        }

        // Perform ! ReadableByteStreamControllerCallPullIfNeeded(controller).
        self.readable_byte_stream_controller_call_pull_if_needed(ctx.clone(), controller, stream)
    }

    fn readable_byte_stream_enqueue_detached_pull_into_to_queue(
        &mut self,
        ctx: Ctx<'js>,
        pull_into_descriptor_index: usize,
    ) -> Result<()> {
        let pull_into_descriptor = &self.pending_pull_intos[pull_into_descriptor_index];
        // If pullIntoDescriptor’s bytes filled > 0, perform ? ReadableByteStreamControllerEnqueueClonedChunkToQueue(controller, pullIntoDescriptor’s buffer, pullIntoDescriptor’s byte offset, pullIntoDescriptor’s bytes filled).
        if pull_into_descriptor.bytes_filled > 0 {
            self.readable_byte_stream_controller_enqueue_cloned_chunk_to_queue(
                ctx,
                pull_into_descriptor.buffer.clone(),
                pull_into_descriptor.byte_offset,
                pull_into_descriptor.bytes_filled,
            )?;
        }

        // Perform ! ReadableByteStreamControllerShiftPendingPullInto(controller).
        self.readable_byte_stream_controller_shift_pending_pull_into();

        Ok(())
    }

    fn readable_byte_stream_controller_process_read_requests_using_queue(
        &mut self,
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
    ) -> Result<()> {
        let stream = self.stream.clone().expect(
            "ReadableByteStreamControllerProcessReadRequestsUsingQueue called without a stream",
        );
        let stream = stream.borrow();

        // Let reader be controller.[[stream]].[[reader]].
        let read_requests = match stream.reader {
            Some(ReadableStreamReader::ReadableStreamDefaultReader(ref r)) => {&mut r.borrow_mut().read_requests},
            _ => panic!("ReadableByteStreamControllerProcessReadRequestsUsingQueue must be called with a stream that has a reader implementing ReadableStreamDefaultReader"),
        };

        // While reader.[[readRequests]] is not empty,
        while !read_requests.is_empty() {
            // If controller.[[queueTotalSize]] is 0, return.
            if self.queue_total_size == 0 {
                return Ok(());
            }

            // Let readRequest be reader.[[readRequests]][0].
            // Remove readRequest from reader.[[readRequests]].
            let read_request = read_requests.pop_front().unwrap();
            // Perform ! ReadableByteStreamControllerFillReadRequestFromQueue(controller, readRequest).
            self.readable_byte_stream_controller_fill_read_request_from_queue(
                ctx,
                controller.clone(),
                read_request,
            )?;
        }

        Ok(())
    }

    fn readable_byte_stream_controller_shift_pending_pull_into(
        &mut self,
    ) -> PullIntoDescriptor<'js> {
        // Let descriptor be controller.[[pendingPullIntos]][0].
        // Remove descriptor from controller.[[pendingPullIntos]].
        // Return descriptor.
        self.pending_pull_intos.pop_front().expect(
            "ReadableByteStreamControllerShiftPendingPullInto called on empty pendingPullIntos",
        )
    }

    fn readable_byte_stream_controller_enqueue_chunk_to_queue(
        &mut self,
        buffer: ArrayBuffer<'js>,
        byte_offset: usize,
        byte_length: usize,
    ) {
        let len = buffer.len();
        // Append a new readable byte stream queue entry with buffer buffer, byte offset byteOffset, and byte length byteLength to controller.[[queue]].
        self.queue.push_back(ReadableByteStreamQueueEntry {
            buffer,
            byte_offset,
            byte_length,
        });

        // Set controller.[[queueTotalSize]] to controller.[[queueTotalSize]] + byteLength.
        self.queue_total_size += len;
    }

    fn readable_byte_stream_controller_process_pull_into_descriptors_using_queue(
        &mut self,
        ctx: &Ctx<'js>,
    ) -> Result<()> {
        // While controller.[[pendingPullIntos]] is not empty,
        while !self.pending_pull_intos.is_empty() {
            // If controller.[[queueTotalSize]] is 0, return.
            if self.queue_total_size == 0 {
                return Ok(());
            }

            // Let pullIntoDescriptor be controller.[[pendingPullIntos]][0].
            let pull_into_descriptor_ref = PullIntoDescriptorRef::Index(0);

            // If ! ReadableByteStreamControllerFillPullIntoDescriptorFromQueue(controller, pullIntoDescriptor) is true,
            if self.readable_byte_stream_controller_fill_pull_into_descriptor_from_queue(
                ctx,
                pull_into_descriptor_ref,
            )? {
                // Perform ! ReadableByteStreamControllerShiftPendingPullInto(controller).
                let pull_into_descriptor =
                    self.readable_byte_stream_controller_shift_pending_pull_into();

                // Perform ! ReadableByteStreamControllerCommitPullIntoDescriptor(controller.[[stream]], pullIntoDescriptor).
                Self::readable_byte_stream_controller_commit_pull_into_descriptor(
                    ctx.clone(),
                    self.stream.clone().expect("ReadableByteStreamControllerProcessPullIntoDescriptorsUsingQueue called without stream"),
                    pull_into_descriptor,
                )?;
            }
        }
        Ok(())
    }

    fn readable_byte_stream_controller_enqueue_cloned_chunk_to_queue(
        &mut self,
        ctx: Ctx<'js>,
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
                self.readable_byte_stream_controller_error(&ctx, ctx.catch())?;
                return Err(err);
            },
        };

        self.readable_byte_stream_controller_enqueue_chunk_to_queue(
            clone_result,
            byte_offset,
            byte_length,
        );

        Ok(())
    }

    fn readable_byte_stream_controller_fill_read_request_from_queue(
        &mut self,
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        let entry = {
            // Assert: controller.[[queueTotalSize]] > 0.
            // Let entry be controller.[[queue]][0].
            // Remove entry from controller.[[queue]].
            let entry = self.queue.pop_front().expect(
                "ReadableByteStreamControllerFillReadRequestFromQueue called with empty queue",
            );

            // Set controller.[[queueTotalSize]] to controller.[[queueTotalSize]] − entry’s byte length.
            self.queue_total_size -= entry.byte_length;

            entry
        };

        // Perform ! ReadableByteStreamControllerHandleQueueDrain(controller).
        self.readable_byte_stream_controller_handle_queue_drain(ctx, controller)?;

        // Let view be ! Construct(%Uint8Array%, « entry’s buffer, entry’s byte offset, entry’s byte length »).
        let ctor: Constructor = ctx.globals().get(PredefinedAtom::Uint8Array)?;
        let view: TypedArray<u8> =
            ctor.construct((entry.buffer, entry.byte_offset, entry.byte_length))?;

        // Perform readRequest’s chunk steps, given view.
        read_request.chunk_steps.call((view,))?;

        Ok(())
    }

    fn readable_byte_stream_controller_fill_pull_into_descriptor_from_queue<'a>(
        &mut self,
        ctx: &Ctx<'js>,
        pull_into_descriptor_ref: PullIntoDescriptorRef<'js, 'a>,
    ) -> Result<bool> {
        let pull_into_descriptor = match &pull_into_descriptor_ref {
            PullIntoDescriptorRef::Index(i) => &self.pending_pull_intos[*i],
            PullIntoDescriptorRef::Owned(d) => d,
        };
        // Let maxBytesToCopy be min(controller.[[queueTotalSize]], pullIntoDescriptor’s byte length − pullIntoDescriptor’s bytes filled).
        let max_bytes_to_copy = std::cmp::min(
            self.queue_total_size,
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
        let queue = &mut self.queue;
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
                ctx,
                pull_into_descriptor.buffer.clone(),
                dest_start,
                head_of_queue.buffer.clone(),
                head_of_queue.byte_offset,
                bytes_to_copy,
            )?;
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
            self.queue_total_size -= bytes_to_copy;

            // Set totalBytesToCopyRemaining to totalBytesToCopyRemaining − bytesToCopy.
            total_bytes_to_copy_remaining -= bytes_to_copy
        }

        Ok(ready)
    }

    fn readable_byte_stream_controller_commit_pull_into_descriptor(
        ctx: Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
        pull_into_descriptor: PullIntoDescriptor<'js>,
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
            ctx.clone(),
            &pull_into_descriptor,
        )?;

        if let PullIntoDescriptorReaderType::Default = pull_into_descriptor.reader_type {
            // If pullIntoDescriptor’s reader type is "default",
            // Perform ! ReadableStreamFulfillReadRequest(stream, filledView, done).
            stream
                .borrow()
                .readable_stream_fulfill_read_request(filled_view.into_js(&ctx)?, done)?
        } else {
            // Otherwise,
            // Perform ! ReadableStreamFulfillReadIntoRequest(stream, filledView, done).
            stream
                .borrow()
                .readable_stream_fulfill_read_into_request(filled_view, done)?
        }

        Ok(())
    }

    fn readable_byte_stream_controller_handle_queue_drain(
        &mut self,
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
    ) -> Result<()> {
        let stream = self
            .stream
            .clone()
            .expect("ReadableByteStreamControllerHandleQueueDrain called without stream");

        // If controller.[[queueTotalSize]] is 0 and controller.[[closeRequested]] is true,
        if self.queue_total_size == 0 && self.close_requested {
            // Perform ! ReadableByteStreamControllerClearAlgorithms(controller).
            self.readable_byte_stream_controller_clear_algorithms();
            // Perform ! ReadableStreamClose(controller.[[stream]]).
            stream.borrow_mut().readable_stream_close()?;
        } else {
            // Otherwise,
            // Perform ! ReadableByteStreamControllerCallPullIfNeeded(controller).
            self.readable_byte_stream_controller_call_pull_if_needed(
                ctx.clone(),
                controller,
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

    pub(super) fn pull_steps(
        &mut self,
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
        read_request: ReadableStreamReadRequest<'js>,
    ) -> Result<()> {
        // Let stream be this.[[stream]].
        let stream = self
            .stream
            .clone()
            .expect("ReadableStreamByteController pullSteps called without stream");

        // If this.[[queueTotalSize]] > 0,
        if self.queue_total_size > 0 {
            // Perform ! ReadableByteStreamControllerFillReadRequestFromQueue(this, readRequest).
            self.readable_byte_stream_controller_fill_read_request_from_queue(
                ctx,
                controller,
                read_request,
            )?;
            // Return.
            return Ok(());
        }

        // Let autoAllocateChunkSize be this.[[autoAllocateChunkSize]].
        let auto_allocate_chunk_size = self.auto_allocate_chunk_size;

        // If autoAllocateChunkSize is not undefined,
        if let Some(auto_allocate_chunk_size) = auto_allocate_chunk_size {
            // Let buffer be Construct(%ArrayBuffer%, « autoAllocateChunkSize »).
            let ctor: Constructor = ctx.globals().get(PredefinedAtom::ArrayBuffer)?;
            let buffer: ArrayBuffer = match ctor.construct((auto_allocate_chunk_size,)) {
                // If buffer is an abrupt completion,
                Err(Error::Exception) => {
                    // Perform readRequest’s error steps, given buffer.[[Value]].
                    read_request.error_steps.call((ctx.catch(),))?;
                    return Ok(());
                },
                Err(err) => return Err(err),
                Ok(buffer) => buffer,
            };

            // Let pullIntoDescriptor be a new pull-into descriptor with...
            let pull_into_descriptor = PullIntoDescriptor {
                buffer,
                buffer_byte_length: auto_allocate_chunk_size,
                byte_offset: 0,
                byte_length: auto_allocate_chunk_size,
                bytes_filled: 0,
                minimum_fill: 1,
                element_size: 1,
                view_constructor: ctx.globals().get(PredefinedAtom::Uint8Array)?,
                reader_type: PullIntoDescriptorReaderType::Default,
            };

            // Append pullIntoDescriptor to this.[[pendingPullIntos]].
            self.pending_pull_intos.push_back(pull_into_descriptor);
        }

        // Perform ! ReadableStreamAddReadRequest(stream, readRequest).
        stream
            .borrow()
            .readable_stream_add_read_request(read_request);

        // Perform ! ReadableByteStreamControllerCallPullIfNeeded(this).
        self.readable_byte_stream_controller_call_pull_if_needed(ctx.clone(), controller, stream)?;

        Ok(())
    }

    pub(super) fn cancel_steps(
        &mut self,
        ctx: &Ctx<'js>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        // Perform ! ReadableByteStreamControllerClearPendingPullIntos(this).
        self.readable_byte_stream_controller_clear_pending_pull_intos();

        // Perform ! ResetQueue(this).
        self.reset_queue();

        // Let result be the result of performing this.[[cancelAlgorithm]], passing in reason.
        let result = match self.cancel_algorithm {
            None => {
                panic!("cancel algorithm used after ReadableStreamByteControllerClearAlgorithms")
            },
            Some(CancelAlgorithm::ReturnPromiseUndefined) => {
                promise_resolved_with(ctx, Ok(Value::new_undefined(ctx.clone())))?
            },
            Some(CancelAlgorithm::Function {
                ref f,
                ref underlying_source,
            }) => f.call((This(underlying_source.clone()), reason))?,
        };

        // Perform ! ReadableByteStreamControllerClearAlgorithms(this).
        self.readable_byte_stream_controller_clear_algorithms();

        // Return result.
        Ok(result)
    }

    pub(super) fn release_steps(&mut self) {
        // If this.[[pendingPullIntos]] is not empty,
        if !self.pending_pull_intos.is_empty() {
            // Let firstPendingPullInto be this.[[pendingPullIntos]][0].
            let first_pending_pull_into = &mut self.pending_pull_intos[0];

            // Set firstPendingPullInto’s reader type to "none".
            first_pending_pull_into.reader_type = PullIntoDescriptorReaderType::None;

            // Set this.[[pendingPullIntos]] to the list « firstPendingPullInto ».
            _ = self.pending_pull_intos.split_off(1);
        }
    }

    pub(super) fn readable_byte_stream_controller_pull_into(
        &mut self,
        ctx: &Ctx<'js>,
        controller: Class<'js, Self>,
        view: ObjectBytes<'js>,
        min: usize,
        read_into_request: ReadableStreamReadIntoRequest<'js>,
    ) -> Result<()> {
        // Let stream be controller.[[stream]].
        let stream = self
            .stream
            .clone()
            .expect("ReadableByteStreamControllerPullInto called without stream");

        // Set elementSize to the element size specified in the typed array constructors table for view.[[TypedArrayName]].
        let (element_size, atom) = match view {
            ObjectBytes::U8Array(_) => (1, PredefinedAtom::Uint8Array),
            ObjectBytes::I8Array(_) => (1, PredefinedAtom::Int8Array),
            ObjectBytes::U16Array(_) => (2, PredefinedAtom::Uint16Array),
            ObjectBytes::I16Array(_) => (2, PredefinedAtom::Int16Array),
            ObjectBytes::U32Array(_) => (4, PredefinedAtom::Uint32Array),
            ObjectBytes::I32Array(_) => (4, PredefinedAtom::Int32Array),
            ObjectBytes::U64Array(_) => (8, PredefinedAtom::BigUint64Array),
            ObjectBytes::I64Array(_) => (8, PredefinedAtom::BigInt64Array),
            ObjectBytes::F32Array(_) => (4, PredefinedAtom::Float32Array),
            ObjectBytes::F64Array(_) => (8, PredefinedAtom::Float64Array),
            ObjectBytes::DataView(_) => (1, PredefinedAtom::DataView),
            ObjectBytes::Vec(_) => panic!("ReadableByteStreamControllerPullInto called with view that is neither typed array nor dataview"),
        };

        // Set ctor to the constructor specified in the typed array constructors table for view.[[TypedArrayName]].
        let ctor: Constructor = ctx.globals().get(atom)?;

        // Let minimumFill be min × elementSize.
        let minimum_fill = min * element_size;

        // Let byteOffset be view.[[ByteOffset]].
        // Let byteLength be view.[[ByteLength]].
        let (buffer, byte_length, byte_offset) = view.get_array_buffer()?.unwrap();

        // Let bufferResult be TransferArrayBuffer(view.[[ViewedArrayBuffer]]).
        let buffer_result = transfer_array_buffer(ctx.clone(), buffer);
        let buffer = match buffer_result {
            // If bufferResult is an abrupt completion,
            Err(Error::Exception) => {
                // Perform readIntoRequest’s error steps, given bufferResult.[[Value]].
                read_into_request.error_steps.call((ctx.catch(),))?;
                // Return.
                return Ok(());
            },
            Err(err) => return Err(err),
            // Let buffer be bufferResult.[[Value]].
            Ok(buffer) => buffer,
        };

        let buffer_byte_length = buffer.len();
        // Let pullIntoDescriptor be a new pull-into descriptor with
        let pull_into_descriptor = PullIntoDescriptor {
            buffer,
            buffer_byte_length,
            byte_offset,
            byte_length,
            bytes_filled: 0,
            minimum_fill,
            element_size,
            view_constructor: ctor.clone(),
            reader_type: PullIntoDescriptorReaderType::Byob,
        };

        // If controller.[[pendingPullIntos]] is not empty,
        if !self.pending_pull_intos.is_empty() {
            // Append pullIntoDescriptor to controller.[[pendingPullIntos]].
            self.pending_pull_intos.push_back(pull_into_descriptor);

            // Perform ! ReadableStreamAddReadIntoRequest(stream, readIntoRequest).
            stream
                .borrow_mut()
                .readable_stream_add_read_into_request(read_into_request);

            // Return.
            return Ok(());
        }

        // If stream.[[state]] is "closed",
        if let ReadableStreamState::Closed = stream.borrow().state {
            // Let emptyView be ! Construct(ctor, « pullIntoDescriptor’s buffer, pullIntoDescriptor’s byte offset, 0 »).
            let empty_view: Value<'js> = ctor.call((
                pull_into_descriptor.buffer,
                pull_into_descriptor.byte_offset,
                0,
            ))?;

            // Perform readIntoRequest’s close steps, given emptyView.
            read_into_request.close_steps.call((empty_view,))?;

            // Return.
            return Ok(());
        }

        // If controller.[[queueTotalSize]] > 0,
        if self.queue_total_size > 0 {
            // If ! ReadableByteStreamControllerFillPullIntoDescriptorFromQueue(controller, pullIntoDescriptor) is true,
            if self.readable_byte_stream_controller_fill_pull_into_descriptor_from_queue(
                ctx,
                PullIntoDescriptorRef::Owned(&pull_into_descriptor),
            )? {
                // Let filledView be ! ReadableByteStreamControllerConvertPullIntoDescriptor(pullIntoDescriptor).
                let filled_view = self.readable_byte_steam_controller_convert_pull_into_descriptor(
                    ctx.clone(),
                    pull_into_descriptor,
                );

                // Perform ! ReadableByteStreamControllerHandleQueueDrain(controller).
                self.readable_byte_stream_controller_handle_queue_drain(ctx, controller)?;

                // Perform readIntoRequest’s chunk steps, given filledView.
                read_into_request.chunk_steps.call((filled_view,))?;

                // Return.
                return Ok(());
            }

            // If controller.[[closeRequested]] is true,
            if self.close_requested {
                // Let e be a TypeError exception.
                let e: Value = ctx.eval(
                    r#"new TypeError("Insufficient bytes to fill elements in the given buffer")"#,
                )?;

                // Perform ! ReadableByteStreamControllerError(controller, e).
                self.readable_byte_stream_controller_error(ctx, e.clone())?;

                // Perform readIntoRequest’s error steps, given e.
                read_into_request.error_steps.call((e,))?;

                // Return.
                return Ok(());
            }
        }

        // Append pullIntoDescriptor to controller.[[pendingPullIntos]].
        self.pending_pull_intos.push_back(pull_into_descriptor);

        // Perform ! ReadableStreamAddReadIntoRequest(stream, readIntoRequest).
        stream
            .borrow_mut()
            .readable_stream_add_read_into_request(read_into_request);

        // Perform ! ReadableByteStreamControllerCallPullIfNeeded(controller).
        self.readable_byte_stream_controller_call_pull_if_needed(ctx.clone(), controller, stream)?;

        Ok(())
    }

    fn readable_byte_steam_controller_convert_pull_into_descriptor(
        &mut self,
        ctx: Ctx<'js>,
        pull_into_descriptor: PullIntoDescriptor<'js>,
    ) -> Result<Value<'js>> {
        // Let bytesFilled be pullIntoDescriptor’s bytes filled.
        let bytes_filled = pull_into_descriptor.bytes_filled;

        // Let elementSize be pullIntoDescriptor’s element size.
        let element_size = pull_into_descriptor.element_size;

        // Let buffer be ! TransferArrayBuffer(pullIntoDescriptor’s buffer).
        let buffer = transfer_array_buffer(ctx, pull_into_descriptor.buffer)?;

        // Return ! Construct(pullIntoDescriptor’s view constructor, « buffer, pullIntoDescriptor’s byte offset, bytesFilled ÷ elementSize »).
        pull_into_descriptor.view_constructor.call((
            buffer,
            pull_into_descriptor.byte_offset,
            bytes_filled / element_size,
        ))
    }
}

#[rquickjs::methods(rename_all = "camelCase")]
impl<'js> ReadableStreamByteController<'js> {
    // readonly attribute unrestricted double? desiredSize;
    #[qjs(get)]
    fn desired_size(&self) -> Option<usize> {
        self.readable_byte_stream_controller_get_desired_size()
    }

    // undefined close();
    fn close(&mut self, ctx: Ctx<'js>) -> Result<()> {
        // If this.[[closeRequested]] is true, throw a TypeError exception.
        if self.close_requested {
            return Err(Exception::throw_type(&ctx, "close() called more than once"));
        }
        match self.stream {
            Some(ref stream) if stream.borrow().state == ReadableStreamState::Readable => {},
            // If this.[[stream]].[[state]] is not "readable", throw a TypeError exception.
            _ => {
                return Err(Exception::throw_type(
                    &ctx,
                    "close() called when stream is not readable",
                ));
            },
        }

        // Perform ? ReadableByteStreamControllerClose(this).
        self.readable_byte_stream_controller_close(&ctx)
    }

    // undefined enqueue(ArrayBufferView chunk);
    fn enqueue(this: This<Class<'js, Self>>, ctx: Ctx<'js>, chunk: Object<'js>) -> Result<()> {
        let mut controller = this.borrow_mut();

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
        if controller.close_requested {
            return Err(Exception::throw_type(&ctx, "stream is closed or draining"));
        }

        // If this.[[stream]].[[state]] is not "readable", throw a TypeError exception.
        if controller
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
        controller.readable_byte_stream_controller_enqueue(&ctx, this.0.clone(), chunk)
    }

    // undefined error(optional any e);
    fn error(&mut self, ctx: Ctx<'js>, e: Value<'js>) -> Result<()> {
        // Perform ! ReadableByteStreamControllerError(this, e).
        self.readable_byte_stream_controller_error(&ctx, e)
    }
}

#[derive(Trace, Clone)]
#[rquickjs::class]
struct ReadableStreamBYOBRequest<'js> {
    view: Option<Value<'js>>,
    controller: Option<Class<'js, ReadableStreamByteController<'js>>>,
}

struct PullIntoDescriptor<'js> {
    buffer: ArrayBuffer<'js>,
    buffer_byte_length: usize,
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

enum PullIntoDescriptorRef<'js, 'a> {
    Index(usize),
    Owned(&'a PullIntoDescriptor<'js>),
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
