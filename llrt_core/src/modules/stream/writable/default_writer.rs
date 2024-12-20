use rquickjs::{
    class::{OwnedBorrow, OwnedBorrowMut, Trace},
    prelude::This,
    Class, Ctx, Exception, Promise, Result, Value,
};

use crate::modules::stream::{promise_rejected_with, set_promise_is_handled_to_true, Null};

use super::{
    default_controller::WritableStreamDefaultController, ResolveablePromise, WritableStream,
    WritableStreamState,
};

#[rquickjs::class]
#[derive(Trace)]
pub(super) struct WritableStreamDefaultWriter<'js> {
    pub(super) ready_promise: ResolveablePromise<'js>,
    pub(super) closed_promise: ResolveablePromise<'js>,
    stream: Option<Class<'js, WritableStream<'js>>>,
}

#[rquickjs::methods(rename_all = "camelCase")]
impl<'js> WritableStreamDefaultWriter<'js> {
    #[qjs(constructor)]
    fn new(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
    ) -> Result<Class<'js, Self>> {
        // Perform ? SetUpWritableStreamDefaultWriter(this, stream).
        let (_, writer) = Self::set_up_writable_stream_default_writer(&ctx, stream)?;
        Ok(writer)
    }

    #[qjs(get)]
    fn closed(writer: This<OwnedBorrowMut<'js, Self>>) -> Promise<'js> {
        // Return this.[[closedPromise]].
        writer.0.closed_promise.promise().clone()
    }

    #[qjs(get)]
    fn desired_size(ctx: Ctx<'js>, writer: This<OwnedBorrowMut<'js, Self>>) -> Result<Null<f64>> {
        match writer.0.stream {
            // If this.[[stream]] is undefined, throw a TypeError exception.
            None => Err(Exception::throw_type(
                &ctx,
                "Cannot desiredSize a stream using a released writer",
            )),
            Some(ref stream) => {
                // Return ! WritableStreamDefaultWriterGetDesiredSize(this).
                Self::writable_stream_default_writer_get_desired_size(OwnedBorrow::from_class(
                    stream.clone(),
                ))
            },
        }
    }

    #[qjs(get)]
    fn ready(writer: This<OwnedBorrowMut<'js, Self>>) -> Promise<'js> {
        // Return this.[[readyPromise]].
        writer.0.ready_promise.promise().clone()
    }

    fn abort(
        ctx: Ctx<'js>,
        writer: This<OwnedBorrowMut<'js, Self>>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        // If this.[[stream]] is undefined, throw a TypeError exception.
        match writer.0.stream {
            None => Err(Exception::throw_type(
                &ctx,
                "Cannot abort a stream using a released writer",
            )),
            Some(ref stream) => {
                let stream = OwnedBorrowMut::from_class(stream.clone());
                let controller = OwnedBorrowMut::from_class(
                    stream
                        .controller
                        .clone()
                        .expect("Abort called on a writable stream that has no controller"),
                );
                // Return ! WritableStreamDefaultWriterAbort(this, reason).
                Self::writable_stream_default_writer_abort(
                    ctx, stream, controller, writer.0, reason,
                )
            },
        }
    }

    fn close(ctx: Ctx<'js>, writer: This<OwnedBorrowMut<'js, Self>>) -> Result<Promise<'js>> {
        // If this.[[stream]] is undefined, throw a TypeError exception.
        match writer.0.stream {
            None => Err(Exception::throw_type(
                &ctx,
                "Cannot close a stream using a released writer",
            )),
            Some(ref stream) => {
                let stream = OwnedBorrowMut::from_class(stream.clone());
                // If ! WritableStreamCloseQueuedOrInFlight(stream) is true, return a promise rejected with a TypeError exception.
                if stream.writable_stream_close_queued_or_in_flight() {
                    let e: Value =
                        ctx.eval(r#"new TypeError("Cannot close an already-closing stream")"#)?;

                    return promise_rejected_with(&ctx, e);
                }

                let controller = OwnedBorrowMut::from_class(
                    stream
                        .controller
                        .clone()
                        .expect("Close called on a writable stream that has no controller"),
                );
                // Return ! WritableStreamDefaultWriterClose(this).
                Self::writable_stream_default_writer_close(ctx, stream, controller, writer.0)
            },
        }
    }

    fn release_lock(ctx: Ctx<'js>, writer: This<OwnedBorrowMut<'js, Self>>) -> Result<()> {
        // If this.[[stream]] is undefined, throw a TypeError exception.
        match writer.0.stream {
            // If stream is undefined, return.
            None => Ok(()),
            Some(ref stream) => {
                let stream = OwnedBorrowMut::from_class(stream.clone());
                // Perform ! WritableStreamDefaultWriterRelease(this).
                Self::writable_stream_default_writer_release(ctx, stream, writer.0)
            },
        }
    }

    fn write(
        ctx: Ctx<'js>,
        writer: This<OwnedBorrowMut<'js, Self>>,
        chunk: Value<'js>,
    ) -> Result<Promise<'js>> {
        // If this.[[stream]] is undefined, throw a TypeError exception.
        match writer.0.stream {
            None => Err(Exception::throw_type(
                &ctx,
                "Cannot write a stream using a released writer",
            )),
            Some(ref stream) => {
                let stream = OwnedBorrowMut::from_class(stream.clone());
                let controller = OwnedBorrowMut::from_class(
                    stream
                        .controller
                        .clone()
                        .expect("Write called on a writable stream that has no controller"),
                );
                // Return ! WritableStreamDefaultWriterWrite(this, chunk).
                Self::writable_stream_default_writer_write(ctx, stream, controller, writer.0, chunk)
            },
        }
    }
}

impl<'js> WritableStreamDefaultWriter<'js> {
    pub(super) fn acquire_writable_stream_default_writer(
        ctx: &Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
    ) -> Result<(OwnedBorrowMut<'js, WritableStream<'js>>, Class<'js, Self>)> {
        Self::set_up_writable_stream_default_writer(ctx, stream)
    }

    pub(super) fn set_up_writable_stream_default_writer(
        ctx: &Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, WritableStream<'js>>,
    ) -> Result<(OwnedBorrowMut<'js, WritableStream<'js>>, Class<'js, Self>)> {
        // If ! IsWritableStreamLocked(stream) is true, throw a TypeError exception.
        if stream.is_writable_stream_locked() {
            return Err(Exception::throw_type(
                ctx,
                "This stream has already been locked for exclusive writing by another writer",
            ));
        }

        let stream_class = stream.into_inner();
        stream = OwnedBorrowMut::from_class(stream_class.clone());

        let (ready_promise, closed_promise) = match stream.state {
            WritableStreamState::Writable => {
                let ready_promise = if !stream.writable_stream_close_queued_or_in_flight()
                    && stream.backpressure
                {
                    // If ! WritableStreamCloseQueuedOrInFlight(stream) is false and stream.[[backpressure]] is true, set writer.[[readyPromise]] to a new promise.
                    ResolveablePromise::new(ctx)?
                } else {
                    // Otherwise, set writer.[[readyPromise]] to a promise resolved with undefined.
                    ResolveablePromise::resolved_with(ctx, Ok(Value::new_undefined(ctx.clone())))?
                };

                // Set writer.[[closedPromise]] to a new promise.
                (ready_promise, ResolveablePromise::new(ctx)?)
            },
            WritableStreamState::Erroring => {
                let stored_error = stream
                    .stored_error
                    .clone()
                    .expect("stream in error state without stored error");
                let ready_promise = ResolveablePromise::rejected_with(ctx, stored_error)?;
                set_promise_is_handled_to_true(ctx.clone(), ready_promise.promise())?;
                // Set writer.[[closedPromise]] to a new promise.
                (ready_promise, ResolveablePromise::new(ctx)?)
            },
            WritableStreamState::Closed => {
                let promise =
                    ResolveablePromise::resolved_with(ctx, Ok(Value::new_undefined(ctx.clone())))?;
                // Set writer.[[readyPromise]] to a promise resolved with undefined.
                // Set writer.[[closedPromise]] to a promise resolved with undefined.
                (promise.clone(), promise)
            },
            WritableStreamState::Errored => {
                // Let storedError be stream.[[storedError]].
                let stored_error = stream
                    .stored_error
                    .clone()
                    .expect("stream in error state without stored error");
                let promise = ResolveablePromise::rejected_with(ctx, stored_error)?;
                set_promise_is_handled_to_true(ctx.clone(), promise.promise())?;
                // Set writer.[[readyPromise]] to a promise rejected with storedError.
                // Set writer.[[readyPromise]].[[PromiseIsHandled]] to true.
                // Set writer.[[closedPromise]] to a promise rejected with storedError.
                // Set writer.[[closedPromise]].[[PromiseIsHandled]] to true.
                (promise.clone(), promise)
            },
        };

        let writer = Self {
            ready_promise,
            closed_promise,
            // Set writer.[[stream]] to stream.
            stream: Some(stream_class),
        };

        let writer = Class::instance(ctx.clone(), writer)?;

        stream.writer = Some(writer.clone());

        Ok((stream, writer))
    }

    pub(super) fn writable_stream_default_writer_ensure_ready_promise_rejected(
        &mut self,
        ctx: &Ctx<'js>,
        error: Value<'js>,
    ) -> Result<()> {
        if self.ready_promise.is_pending() {
            // If writer.[[readyPromise]].[[PromiseState]] is "pending", reject writer.[[readyPromise]] with error.
            self.ready_promise.reject(error)?;
        } else {
            // Otherwise, set writer.[[readyPromise]] to a promise rejected with error.
            self.ready_promise = ResolveablePromise::rejected_with(ctx, error)?;
        }

        // Set writer.[[readyPromise]].[[PromiseIsHandled]] to true.
        set_promise_is_handled_to_true(ctx.clone(), &self.ready_promise.promise())?;
        Ok(())
    }

    pub(super) fn writable_stream_default_writer_ensure_closed_promise_rejected(
        &mut self,
        ctx: &Ctx<'js>,
        error: Value<'js>,
    ) -> Result<()> {
        if self.closed_promise.is_pending() {
            // If writer.[[closedPromise]].[[PromiseState]] is "pending", reject writer.[[closedPromise]] with error.
            self.closed_promise.reject(error)?;
        } else {
            // Otherwise, set writer.[[closedPromise]] to a promise rejected with error.
            self.closed_promise = ResolveablePromise::rejected_with(ctx, error)?;
        }

        // Set writer.[[closedPromise]].[[PromiseIsHandled]] to true.
        set_promise_is_handled_to_true(ctx.clone(), &self.closed_promise.promise())?;
        Ok(())
    }

    pub(super) fn writable_stream_default_writer_get_desired_size(
        // Let stream be writer.[[stream]].
        stream: OwnedBorrow<'js, WritableStream<'js>>,
    ) -> Result<Null<f64>> {
        // Let state be stream.[[state]].
        let state = stream.state;

        // If state is "errored" or "erroring", return null.
        if matches!(
            state,
            WritableStreamState::Errored | WritableStreamState::Erroring
        ) {
            return Ok(Null(None));
        }

        // If state is "closed", return 0.
        if matches!(state, WritableStreamState::Closed) {
            return Ok(Null(Some(0.0)));
        }

        // Return ! WritableStreamDefaultControllerGetDesiredSize(stream.[[controller]]).
        let controller = OwnedBorrow::from_class(
            stream
                .controller
                .clone()
                .expect("Stream in state writable must have a controller"),
        );

        Ok(Null(Some(
            controller.writable_stream_default_controller_get_desired_size(),
        )))
    }

    fn writable_stream_default_writer_abort(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        controller: OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        writer: OwnedBorrowMut<'js, Self>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        // Return ! WritableStreamAbort(stream, reason).
        let (promise, _, _, _) =
            WritableStream::writable_stream_abort(ctx, stream, controller, Some(writer), reason)?;
        Ok(promise)
    }

    fn writable_stream_default_writer_close(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        controller: OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        writer: OwnedBorrowMut<'js, Self>,
    ) -> Result<Promise<'js>> {
        // Return ! WritableStreamAbort(stream, reason).
        let (promise, _, _, _) =
            WritableStream::writable_stream_close(ctx, stream, controller, Some(writer))?;
        Ok(promise)
    }

    fn writable_stream_default_writer_release(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        mut writer: OwnedBorrowMut<'js, Self>,
    ) -> Result<()> {
        // Let releasedError be a new TypeError.
        let released_error: Value =
            ctx.eval(r#"new TypeError("Writer was released and can no longer be used to monitor the stream's closedness")"#)?;

        // Perform ! WritableStreamDefaultWriterEnsureReadyPromiseRejected(writer, releasedError).
        writer.writable_stream_default_writer_ensure_ready_promise_rejected(
            &ctx,
            released_error.clone(),
        )?;
        // Perform ! WritableStreamDefaultWriterEnsureClosedPromiseRejected(writer, releasedError).
        writer
            .writable_stream_default_writer_ensure_closed_promise_rejected(&ctx, released_error)?;

        // Set stream.[[writer]] to undefined.
        stream.writer = None;
        // Set writer.[[stream]] to undefined.
        writer.stream = None;

        Ok(())
    }

    fn writable_stream_default_writer_write(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        controller: OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        writer: OwnedBorrowMut<'js, Self>,
        chunk: Value<'js>,
    ) -> Result<Promise<'js>> {
        // Let chunkSize be ! WritableStreamDefaultControllerGetChunkSize(controller, chunk).
        let (chunk_size, mut stream, controller, writer) =
            WritableStreamDefaultController::writable_stream_default_controller_get_chunk_size(
                ctx.clone(),
                stream,
                controller,
                writer,
                chunk.clone(),
            )?;

        let stream_class = stream.into_inner();
        stream = OwnedBorrowMut::from_class(stream_class.clone());

        // If stream is not equal to writer.[[stream]], return a promise rejected with a TypeError exception.
        if writer.stream != Some(stream_class) {
            let e: Value =
                ctx.eval(r#"new TypeError("Cannot write to a stream using a released writer")"#)?;

            return promise_rejected_with(&ctx, e);
        }

        // Let state be stream.[[state]].
        let state = stream.state;
        // If state is "errored", return a promise rejected with stream.[[storedError]].
        if matches!(state, WritableStreamState::Errored) {
            return promise_rejected_with(
                &ctx,
                stream
                    .stored_error
                    .clone()
                    .expect("stream in error state without stored error"),
            );
        }

        // If ! WritableStreamCloseQueuedOrInFlight(stream) is true or state is "closed", return a promise rejected with a TypeError exception indicating that the stream is closing or closed.
        if stream.writable_stream_close_queued_or_in_flight()
            || matches!(state, WritableStreamState::Closed)
        {
            let e: Value = ctx.eval(
                r#"new TypeError("The stream is closing or closed and cannot be written to")"#,
            )?;

            return promise_rejected_with(&ctx, e);
        }

        // If state is "erroring", return a promise rejected with stream.[[storedError]].
        if matches!(state, WritableStreamState::Erroring) {
            return promise_rejected_with(
                &ctx,
                stream
                    .stored_error
                    .clone()
                    .expect("stream in erroring state without stored error"),
            );
        }

        // Let promise be ! WritableStreamAddWriteRequest(stream).
        let promise = stream.writable_stream_add_write_request(&ctx);
        // Perform ! WritableStreamDefaultControllerWrite(controller, chunk, chunkSize).
        controller.writable_stream_default_controller_write(chunk, chunk_size)?;

        // Return promise.
        promise
    }
}
