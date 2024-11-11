use std::collections::VecDeque;

use default_controller::WritableStreamDefaultController;
use default_writer::WritableStreamDefaultWriter;
use rquickjs::{
    class::{OwnedBorrowMut, Trace},
    prelude::{Opt, This},
    Class, Ctx, Exception, Function, Object, Promise, Result, Value,
};

use crate::modules::events::abort_controller::AbortController;

use super::{
    promise_rejected_with, promise_resolved_with, set_promise_is_handled_to_true, upon_promise,
    Null, ObjectExt, QueuingStrategy, Undefined,
};

mod default_controller;
mod default_writer;

#[rquickjs::class]
#[derive(Trace)]
pub struct WritableStream<'js> {
    backpressure: bool,
    close_request: Option<ResolveablePromise<'js>>,
    controller: Option<Class<'js, WritableStreamDefaultController<'js>>>,
    in_flight_write_request: Option<ResolveablePromise<'js>>,
    in_flight_close_request: Option<ResolveablePromise<'js>>,
    pending_abort_request: Option<PendingAbortRequest<'js>>,
    state: WritableStreamState,
    stored_error: Option<Value<'js>>,
    writer: Option<Class<'js, WritableStreamDefaultWriter<'js>>>,
    write_requests: VecDeque<ResolveablePromise<'js>>,
}

#[derive(Clone)]
struct ResolveablePromise<'js> {
    promise: Promise<'js>,
    resolve: Function<'js>,
    reject: Function<'js>,
}

impl<'js> ResolveablePromise<'js> {
    fn new(ctx: &Ctx<'js>) -> Result<Self> {
        let (promise, resolve, reject) = Promise::new(ctx)?;
        Ok(Self {
            promise,
            resolve,
            reject,
        })
    }
}

impl<'js> Trace<'js> for ResolveablePromise<'js> {
    fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
        self.promise.trace(tracer);
        self.resolve.trace(tracer);
        self.reject.trace(tracer);
    }
}

#[rquickjs::methods(rename_all = "camelCase")]
impl<'js> WritableStream<'js> {
    // constructor(optional object underlyingSink, optional QueuingStrategy strategy = {});
    #[qjs(constructor)]
    fn new(
        ctx: Ctx<'js>,
        underlying_sink: Opt<Undefined<Object<'js>>>,
        queuing_strategy: Opt<Undefined<QueuingStrategy<'js>>>,
    ) -> Result<Class<'js, Self>> {
        // If underlyingSink is missing, set it to null.
        let underlying_sink = Null(underlying_sink.0);

        // Let underlyingSinkDict be underlyingSink, converted to an IDL value of type UnderlyingSink.
        let underlying_sink_dict = match underlying_sink {
            Null(None) | Null(Some(Undefined(None))) => UnderlyingSink::default(),
            Null(Some(Undefined(Some(ref obj)))) => UnderlyingSink::from_object(obj.clone())?,
        };

        // If underlyingSinkDict["type"] exists, throw a RangeError exception.
        if underlying_sink_dict.r#type.is_some() {
            return Err(Exception::throw_range(&ctx, "Invalid type is specified"));
        }

        // Perform ! InitializeWritableStream(this).
        let stream_class = Class::instance(
            ctx.clone(),
            Self {
                // Set stream.[[state]] to "writable".
                state: WritableStreamState::Writable,
                // Set stream.[[storedError]], stream.[[writer]], stream.[[controller]], stream.[[inFlightWriteRequest]], stream.[[closeRequest]], stream.[[inFlightCloseRequest]], and stream.[[pendingAbortRequest]] to undefined.
                stored_error: None,
                writer: None,
                controller: None,
                in_flight_write_request: None,
                close_request: None,
                in_flight_close_request: None,
                pending_abort_request: None,
                // Set stream.[[writeRequests]] to a new empty list.
                write_requests: VecDeque::new(),
                // Set stream.[[backpressure]] to false.
                backpressure: false,
            },
        )?;
        let stream = OwnedBorrowMut::from_class(stream_class.clone());
        let queuing_strategy = queuing_strategy.0.and_then(|qs| qs.0);

        // Let sizeAlgorithm be ! ExtractSizeAlgorithm(strategy).
        let size_algorithm = QueuingStrategy::extract_size_algorithm(queuing_strategy.as_ref());

        // Let highWaterMark be ? ExtractHighWaterMark(strategy, 1).
        let high_water_mark =
            QueuingStrategy::extract_high_water_mark(&ctx, queuing_strategy, 1.0)?;

        // Perform ? SetUpWritableStreamDefaultControllerFromUnderlyingSink(this, underlyingSink, underlyingSinkDict, highWaterMark, sizeAlgorithm).
        WritableStreamDefaultController::set_up_writable_stream_default_controller_from_underlying_sink(ctx, stream, underlying_sink, underlying_sink_dict, high_water_mark, size_algorithm)?;

        Ok(stream_class)
    }

    // readonly attribute boolean locked;
    #[qjs(get)]
    fn locked(&self) -> bool {
        // Return ! IsWritableStreamLocked(this).
        self.is_writable_stream_locked()
    }

    fn abort(
        ctx: Ctx<'js>,
        mut stream: This<OwnedBorrowMut<'js, Self>>,
        reason: Opt<Value<'js>>,
    ) -> Result<Promise<'js>> {
        if stream.is_writable_stream_locked() {
            // If ! IsWritableStreamLocked(this) is true, return a promise rejected with a TypeError exception.
            let e: Value =
                ctx.eval(r#"new TypeError("Cannot abort a stream that already has a writer")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        let controller = OwnedBorrowMut::from_class(
            stream
                .controller
                .clone()
                .expect("Abort called on stream without a controller"),
        );
        let writer = stream.writer_mut();

        // Return ! WritableStreamAbort(this, reason).
        let (promise, _, _, _) = Self::writable_stream_abort(
            ctx.clone(),
            stream.0,
            controller,
            writer,
            reason.0.unwrap_or(Value::new_undefined(ctx)),
        )?;

        Ok(promise)
    }

    fn close(ctx: Ctx<'js>, mut stream: This<OwnedBorrowMut<'js, Self>>) -> Result<Promise<'js>> {
        if stream.is_writable_stream_locked() {
            // If ! IsWritableStreamLocked(this) is true, return a promise rejected with a TypeError exception.
            let e: Value =
                ctx.eval(r#"new TypeError("Cannot close a stream that already has a writer")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        if Self::writable_stream_close_queued_or_in_flight(&stream.0) {
            // If ! WritableStreamCloseQueuedOrInFlight(this) is true, return a promise rejected with a TypeError exception.
            let e: Value =
                ctx.eval(r#"new TypeError("Cannot close an already-closing stream")"#)?;
            return promise_rejected_with(&ctx, e);
        }

        let controller = OwnedBorrowMut::from_class(
            stream
                .controller
                .clone()
                .expect("Abort called on stream without a controller"),
        );
        let writer = stream.writer_mut();

        // Return ! WritableStreamClose(this).
        let (promise, _, _, _) =
            Self::writable_stream_close(ctx.clone(), stream.0, controller, writer)?;

        Ok(promise)
    }

    fn get_writer(
        ctx: Ctx<'js>,
        stream: This<OwnedBorrowMut<'js, Self>>,
    ) -> Result<Class<'js, WritableStreamDefaultWriter<'js>>> {
        // Return ? AcquireWritableStreamDefaultWriter(this).
        let (_, writer) =
            WritableStreamDefaultWriter::acquire_writable_stream_default_writer(&ctx, stream.0)?;

        Ok(writer)
    }
}

impl<'js> WritableStream<'js> {
    fn is_writable_stream_locked(&self) -> bool {
        if self.writer.is_none() {
            // If stream.[[writer]] is undefined, return false.
            false
        } else {
            // Return true.
            true
        }
    }

    fn writable_stream_abort(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        mut writer: Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
        mut reason: Value<'js>,
    ) -> Result<(
        Promise<'js>,
        OwnedBorrowMut<'js, Self>,
        OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
    )> {
        // If stream.[[state]] is "closed" or "errored", return a promise resolved with undefined.
        if matches!(
            stream.state,
            WritableStreamState::Closed | WritableStreamState::Errored
        ) {
            return Ok((
                promise_resolved_with(&ctx, Ok(Value::new_undefined(ctx.clone())))?,
                stream,
                controller,
                writer,
            ));
        }

        // Signal abort on stream.[[controller]].[[abortController]] with reason.
        {
            // this executes user code, so we should ensure we hold no locks
            let abort_controller = controller.abort_controller.clone();
            let (stream_class, controller_class, writer_class) = (
                stream.into_inner(),
                controller.into_inner(),
                writer.map(|w| w.into_inner()),
            );
            AbortController::abort(
                ctx.clone(),
                This(abort_controller),
                Opt(Some(reason.clone())),
            )?;
            (stream, controller, writer) = (
                OwnedBorrowMut::from_class(stream_class),
                OwnedBorrowMut::from_class(controller_class),
                writer_class.map(OwnedBorrowMut::from_class),
            )
        }

        // Let state be stream.[[state]].
        let state = stream.state;

        // If state is "closed" or "errored", return a promise resolved with undefined.
        if matches!(
            state,
            WritableStreamState::Closed | WritableStreamState::Errored
        ) {
            return Ok((
                promise_resolved_with(&ctx, Ok(Value::new_undefined(ctx.clone())))?,
                stream,
                controller,
                writer,
            ));
        }

        // If stream.[[pendingAbortRequest]] is not undefined, return stream.[[pendingAbortRequest]]'s promise.
        match stream.pending_abort_request {
            None => {},
            Some(ref pending_abort_request) => {
                return Ok((
                    pending_abort_request.promise.promise.clone(),
                    stream,
                    controller,
                    writer,
                ))
            },
        }

        let was_already_erroring = match state {
            // If state is "erroring",
            // Set wasAlreadyErroring to true.
            // Set reason to undefined.
            WritableStreamState::Erroring => {
                reason = Value::new_undefined(ctx.clone());
                true
            },
            // Let wasAlreadyErroring be false.
            _ => false,
        };

        // Let promise be a new promise.
        let promise = ResolveablePromise::new(&ctx)?;

        // Set stream.[[pendingAbortRequest]] to a new pending abort request whose promise is promise, reason is reason, and was already erroring is wasAlreadyErroring.
        stream.pending_abort_request = Some(PendingAbortRequest {
            promise: promise.clone(),
            reason: reason.clone(),
            was_already_erroring,
        });

        // If wasAlreadyErroring is false, perform ! WritableStreamStartErroring(stream, reason).
        if !was_already_erroring {
            (stream, controller, writer) =
                Self::writable_stream_start_erroring(ctx, stream, controller, writer, reason)?;
        }

        Ok((promise.promise, stream, controller, writer))
    }

    fn writable_stream_close(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        mut writer: Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
    ) -> Result<(
        Promise<'js>,
        OwnedBorrowMut<'js, Self>,
        OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
    )> {
        // Let state be stream.[[state]].
        let state = stream.state;

        // If state is "closed" or "errored", return a promise rejected with a TypeError exception.
        if matches!(
            state,
            WritableStreamState::Closed | WritableStreamState::Errored
        ) {
            let e: Value = ctx.eval(
                r#"new TypeError("The stream is not in the writable state and cannot be closed")"#,
            )?;
            return Ok((promise_rejected_with(&ctx, e)?, stream, controller, writer));
        }

        // Let promise be a new promise.
        let promise = ResolveablePromise::new(&ctx)?;
        // Set stream.[[closeRequest]] to promise.
        stream.close_request = Some(promise.clone());

        // Let writer be stream.[[writer]].
        // If writer is not undefined, and stream.[[backpressure]] is true, and state is "writable", resolve writer.[[readyPromise]] with undefined.
        if let Some(ref writer) = writer {
            if stream.backpressure && matches!(state, WritableStreamState::Writable) {
                let () = writer
                    .ready_promise
                    .resolve
                    .call((Value::new_undefined(ctx.clone()),))?;
            }
        }

        // Perform ! WritableStreamDefaultControllerClose(stream.[[controller]]).
        (stream, controller, writer) =
            WritableStreamDefaultController::writable_stream_default_controller_close(
                ctx, stream, controller, writer,
            )?;

        // Return promise.
        Ok((promise.promise, stream, controller, writer))
    }

    fn writable_stream_start_erroring(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        // Let controller be stream.[[controller]].
        mut controller: OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        // Let writer be stream.[[writer]].
        mut writer: Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
        reason: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, Self>,
        OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
    )> {
        // Set stream.[[state]] to "erroring".
        stream.state = WritableStreamState::Erroring;
        // Set stream.[[storedError]] to reason.
        stream.stored_error = Some(reason.clone());

        // If writer is not undefined, perform ! WritableStreamDefaultWriterEnsureReadyPromiseRejected(writer, reason).
        if let Some(ref mut writer) = writer {
            writer.writable_stream_default_writer_ensure_ready_promise_rejected(&ctx, reason)?;
        }

        // If ! WritableStreamHasOperationMarkedInFlight(stream) is false and controller.[[started]] is true, perform ! WritableStreamFinishErroring(stream).
        if !stream.writable_stream_has_operation_marked_in_flight() && controller.started {
            (stream, controller, writer) =
                Self::writable_stream_finish_erroring(ctx, stream, controller, writer)?;
        }

        Ok((stream, controller, writer))
    }

    fn writable_stream_finish_erroring(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        writer: Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
    ) -> Result<(
        OwnedBorrowMut<'js, Self>,
        OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
    )> {
        // Set stream.[[state]] to "errored".
        stream.state = WritableStreamState::Errored;

        // Perform ! stream.[[controller]].[[ErrorSteps]]().
        controller.error_steps()?;

        // Let storedError be stream.[[storedError]].
        let stored_error = stream
            .stored_error
            .clone()
            .expect("stream in error state without stored error");

        // For each writeRequest of stream.[[writeRequests]]:
        for write_request in &stream.write_requests {
            let () = write_request.reject.call((stored_error.clone(),))?;
        }

        // Set stream.[[writeRequests]] to an empty list.
        stream.write_requests.clear();

        // Let abortRequest be stream.[[pendingAbortRequest]].
        // Set stream.[[pendingAbortRequest]] to undefined.
        let abort_request = if let Some(pending_abort_request) = stream.pending_abort_request.take()
        {
            pending_abort_request
        } else {
            // If stream.[[pendingAbortRequest]] is undefined,
            // Perform ! WritableStreamRejectCloseAndClosedPromiseIfNeeded(stream).
            stream.writable_stream_reject_close_and_closed_promise_if_needed(
                ctx,
                writer.as_deref(),
            )?;
            // Return.
            return Ok((stream, controller, writer));
        };

        // If abortRequest’s was already erroring is true,
        if abort_request.was_already_erroring {
            // Reject abortRequest’s promise with storedError.
            let () = abort_request.promise.reject.call((stored_error.clone(),))?;

            // Perform ! WritableStreamRejectCloseAndClosedPromiseIfNeeded(stream).
            stream.writable_stream_reject_close_and_closed_promise_if_needed(
                ctx,
                writer.as_deref(),
            )?;

            // Return.
            return Ok((stream, controller, writer));
        }

        // Let promise be ! stream.[[controller]].[[AbortSteps]](abortRequest’s reason).
        let promise = controller.abort_steps(abort_request.reason)?;

        let stream_class = stream.into_inner();
        stream = OwnedBorrowMut::from_class(stream_class.clone());

        // Upon fulfillment of promise,
        let _ = upon_promise::<Value<'js>, _>(ctx.clone(), promise, {
            move |ctx, result| {
                let mut stream = OwnedBorrowMut::from_class(stream_class);
                let writer = stream.writer_mut();
                match result {
                    // Upon fulfillment of promise,
                    Ok(_) => {
                        // Resolve abortRequest’s promise with undefined.
                        let () = abort_request
                            .promise
                            .resolve
                            .call((Value::new_undefined(ctx.clone()),))?;
                        // Perform ! WritableStreamRejectCloseAndClosedPromiseIfNeeded(stream).
                        stream.writable_stream_reject_close_and_closed_promise_if_needed(
                            ctx,
                            writer.as_deref(),
                        )
                    },
                    // Upon rejection of promise with reason reason,
                    Err(reason) => {
                        // Reject abortRequest’s promise with reason.
                        let () = abort_request.promise.reject.call((reason,))?;
                        // Perform ! WritableStreamRejectCloseAndClosedPromiseIfNeeded(stream).
                        stream.writable_stream_reject_close_and_closed_promise_if_needed(
                            ctx,
                            writer.as_deref(),
                        )
                    },
                }
            }
        })?;

        Ok((stream, controller, writer))
    }

    fn writable_stream_reject_close_and_closed_promise_if_needed(
        &mut self,
        ctx: Ctx<'js>,
        // Let writer be stream.[[writer]].
        writer: Option<&WritableStreamDefaultWriter<'js>>,
    ) -> Result<()> {
        // If stream.[[closeRequest]] is not undefined,
        if let Some(ref close_request) = self.close_request {
            // Reject stream.[[closeRequest]] with stream.[[storedError]].
            let () = close_request.reject.call((self.stored_error.clone(),))?;
            // Set stream.[[closeRequest]] to undefined.
            self.close_request = None;
        }

        // If writer is not undefined,
        if let Some(writer) = writer {
            // Reject writer.[[closedPromise]] with stream.[[storedError]].
            let () = writer
                .closed_promise
                .reject
                .call((self.stored_error.clone(),))?;

            // Set writer.[[closedPromise]].[[PromiseIsHandled]] to true.
            set_promise_is_handled_to_true(ctx, &writer.closed_promise.promise)?;
        }

        Ok(())
    }

    fn writable_stream_has_operation_marked_in_flight(&self) -> bool {
        if self.in_flight_write_request.is_none() && self.in_flight_close_request.is_none() {
            // If stream.[[inFlightWriteRequest]] is undefined and stream.[[inFlightCloseRequest]] is undefined, return false.
            false
        } else {
            // Return true.
            true
        }
    }

    fn writable_stream_close_queued_or_in_flight(&self) -> bool {
        if self.close_request.is_none() && self.in_flight_close_request.is_none() {
            // If stream.[[closeRequest]] is undefined and stream.[[inFlightCloseRequest]] is undefined, return false.
            false
        } else {
            // Return true.
            true
        }
    }

    fn writer_mut(&mut self) -> Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>> {
        self.writer.clone().map(OwnedBorrowMut::from_class)
    }
}

#[derive(Debug, Trace, Clone, Copy, PartialEq, Eq)]
enum WritableStreamState {
    Writable,
    Closed,
    Erroring,
    Errored,
}

#[derive(Trace)]
struct PendingAbortRequest<'js> {
    promise: ResolveablePromise<'js>,
    reason: Value<'js>,
    was_already_erroring: bool,
}

#[derive(Default)]
struct UnderlyingSink<'js> {
    // callback UnderlyingSinkStartCallback = any (WritableStreamDefaultController controller);
    start: Option<Function<'js>>,
    // callback UnderlyingSinkWriteCallback = Promise<undefined> (any chunk, WritableStreamDefaultController controller);
    write: Option<Function<'js>>,
    // callback UnderlyingSinkCloseCallback = Promise<undefined> ();
    close: Option<Function<'js>>,
    // callback UnderlyingSinkAbortCallback = Promise<undefined> (optional any reason);
    abort: Option<Function<'js>>,
    r#type: Option<Value<'js>>,
}

impl<'js> UnderlyingSink<'js> {
    fn from_object(obj: Object<'js>) -> Result<Self> {
        let start = obj.get_optional::<_, _>("start")?;
        let write = obj.get_optional::<_, _>("write")?;
        let close = obj.get_optional::<_, _>("close")?;
        let abort = obj.get_optional::<_, _>("abort")?;
        let r#type = obj.get_optional::<_, _>("type")?;

        Ok(Self {
            start,
            write,
            close,
            abort,
            r#type,
        })
    }
}
