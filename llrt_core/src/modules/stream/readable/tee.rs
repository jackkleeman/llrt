use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

use rquickjs::{
    atom::PredefinedAtom,
    class::{OwnedBorrowMut, Trace},
    function::Constructor,
    prelude::{List, OnceFn, Opt},
    ArrayBuffer, Class, Ctx, Error, FromJs, Function, IntoJs, Promise, Result, Value,
};

use crate::{
    modules::stream::readable::{
        byob_reader::ReadableStreamReadIntoRequest,
        default_controller::ReadableStreamDefaultController, promise_resolved_with, upon_promise,
        CancelAlgorithm, Null, PullAlgorithm, ReadableByteStreamController,
        ReadableStreamBYOBReader, ReadableStreamDefaultReader, ReadableStreamReadRequest,
        ReadableStreamReader, ReadableStreamReaderOwnedBorrowMut, StartAlgorithm,
    },
    utils::clone::structured_clone,
};

use super::{
    byob_reader::ViewBytes, ReadableStream, ReadableStreamController,
    ReadableStreamControllerOwnedBorrowMut,
};

impl<'js> ReadableStream<'js> {
    pub(super) fn readable_stream_tee(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, Self>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        clone_for_branch_2: bool,
    ) -> Result<(Class<'js, Self>, Class<'js, Self>)> {
        match controller {
            // If stream.[[controller]] implements ReadableByteStreamController, return ? ReadableByteStreamTee(stream).
            ReadableStreamControllerOwnedBorrowMut::ReadableStreamByteController(controller) => {
                Self::readable_byte_stream_tee(ctx, stream, controller)
            },
            // Otherwise,
            ReadableStreamControllerOwnedBorrowMut::ReadableStreamDefaultController(controller) => {
                // Return ? ReadableStreamDefaultTee(stream, cloneForBranch2).
                Self::readable_stream_default_tee(ctx, stream, controller, clone_for_branch_2)
            },
        }
    }

    fn readable_stream_default_tee(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, Self>,
        controller: OwnedBorrowMut<'js, ReadableStreamDefaultController<'js>>,
        clone_for_branch_2: bool,
    ) -> Result<(Class<'js, Self>, Class<'js, Self>)> {
        // Let reader be ? AcquireReadableStreamDefaultReader(stream).
        let (stream, reader) =
            ReadableStreamReader::acquire_readable_stream_default_reader(ctx.clone(), stream)?;
        // Let reading be false.
        let reading = Rc::new(AtomicBool::new(false));
        // Let readAgain be false.
        let read_again = Rc::new(AtomicBool::new(false));
        // Let canceled1 be false.
        // Let canceled2 be false.
        // Let reason1 be undefined.
        let reason_1 = Rc::new(OnceCell::new());
        // Let reason2 be undefined.
        let reason_2 = Rc::new(OnceCell::new());
        // Let branch1 be undefined.
        let branch_1: Rc<OnceCell<Class<'js, Self>>> = Rc::new(OnceCell::new());
        // Let branch2 be undefined.
        let branch_2: Rc<OnceCell<Class<'js, Self>>> = Rc::new(OnceCell::new());
        // Let cancelPromise be a new promise.
        let (cancel_promise, resolve_cancel_promise, reject_cancel_promise) = Promise::new(&ctx)?;

        // Let startAlgorithm be an algorithm that returns undefined.
        let start_algorithm = StartAlgorithm::ReturnUndefined;

        let stream = stream.into_inner();
        let controller = controller.into_inner();

        let pull_algorithm = PullAlgorithm::Function {
            f: Function::new(ctx.clone(), {
                let stream = stream.clone();
                let controller = controller.clone();
                let reader = reader.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let branch_1 = branch_1.clone();
                let branch_2 = branch_2.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                move |ctx: Ctx<'js>,
                      branch_controller: OwnedBorrowMut<
                    'js,
                    ReadableStreamDefaultController<'js>,
                >| {
                    let stream = OwnedBorrowMut::from_class(stream.clone());
                    let reader = OwnedBorrowMut::from_class(reader.clone());
                    let controller = OwnedBorrowMut::from_class(controller.clone());
                    drop(branch_controller);
                    Self::readable_stream_default_pull_algorithm(
                        ctx,
                        stream,
                        controller,
                        clone_for_branch_2,
                        reader,
                        reading.clone(),
                        read_again.clone(),
                        reason_1.clone(),
                        reason_2.clone(),
                        branch_1.clone(),
                        branch_2.clone(),
                        resolve_cancel_promise.clone(),
                        reject_cancel_promise.clone(),
                    )
                }
            })?,
            underlying_source: Null(None),
        };
        let cancel_algorithm_1 = CancelAlgorithm::Function {
            f: Function::new(ctx.clone(), {
                OnceFn::new({
                    let stream = stream.clone();
                    let controller = controller.clone();
                    let reader = reader.clone();
                    let reason_1 = reason_1.clone();
                    let reason_2 = reason_2.clone();
                    let cancel_promise = cancel_promise.clone();
                    let resolve_cancel_promise = resolve_cancel_promise.clone();
                    move |reason: Value<'js>| {
                        let stream = OwnedBorrowMut::from_class(stream);
                        let controller = OwnedBorrowMut::from_class(controller);
                        let reader = OwnedBorrowMut::from_class(reader);
                        Self::readable_stream_cancel_1_algorithm(
                            reason.ctx().clone(),
                            stream,
                            controller.into(),
                            reader.into(),
                            reason_1,
                            reason_2,
                            cancel_promise,
                            resolve_cancel_promise,
                            reason,
                        )
                    }
                })
            })?,
            underlying_source: Null(None),
        };

        let cancel_algorithm_2 = CancelAlgorithm::Function {
            f: Function::new(ctx.clone(), {
                OnceFn::new({
                    let stream = stream.clone();
                    let controller = controller.clone();
                    let reader = reader.clone();
                    let reason_1 = reason_1.clone();
                    let reason_2 = reason_2.clone();
                    let resolve_cancel_promise = resolve_cancel_promise.clone();
                    move |reason: Value<'js>| {
                        let stream = OwnedBorrowMut::from_class(stream);
                        let controller = OwnedBorrowMut::from_class(controller);
                        let reader = OwnedBorrowMut::from_class(reader);
                        Self::readable_stream_cancel_2_algorithm(
                            reason.ctx().clone(),
                            stream,
                            controller.into(),
                            reader.into(),
                            reason_1,
                            reason_2,
                            cancel_promise,
                            resolve_cancel_promise,
                            reason,
                        )
                    }
                })
            })?,
            underlying_source: Null(None),
        };

        // Set branch1 to ! CreateReadableStream(startAlgorithm, pullAlgorithm, cancel1Algorithm).
        let (branch_1, controller_1) = {
            let (s, c) = Self::create_readable_stream(
                ctx.clone(),
                start_algorithm.clone(),
                pull_algorithm.clone(),
                cancel_algorithm_1,
                None,
                None,
            )?;
            _ = branch_1.set(s.clone());
            (s, c)
        };

        // Set branch2 to ! CreateReadableStream(startAlgorithm, pullAlgorithm, cancel2Algorithm).
        let (branch_2, controller_2) = {
            let (s, c) = Self::create_readable_stream(
                ctx.clone(),
                start_algorithm,
                pull_algorithm,
                cancel_algorithm_2,
                None,
                None,
            )?;
            _ = branch_2.set(s.clone());
            (s, c)
        };

        upon_promise(
            ctx.clone(),
            reader.borrow().generic.closed_promise.clone(),
            {
                let branch_1 = branch_1.clone();
                let branch_2 = branch_2.clone();
                move |ctx, result| match result {
                    Ok(()) => Ok(()),
                    // Upon rejection of reader.[[closedPromise]] with reason r,
                    Err(reason) => {
                        // Perform ! ReadableStreamDefaultControllerError(branch1.[[controller]], r).
                        let mut stream_1 = OwnedBorrowMut::from_class(branch_1);
                        let controller_1 = OwnedBorrowMut::from_class(controller_1);
                        let reader_1 = stream_1.reader_mut();
                        ReadableStreamDefaultController::readable_stream_default_controller_error(
                            &ctx,
                            stream_1,
                            controller_1,
                            reader_1,
                            reason.clone(),
                        )?;

                        // Perform ! ReadableStreamDefaultControllerError(branch2.[[controller]], r).
                        let mut stream_2 = OwnedBorrowMut::from_class(branch_2);
                        let controller_2 = OwnedBorrowMut::from_class(controller_2);
                        let reader_2 = stream_2.reader_mut();
                        ReadableStreamDefaultController::readable_stream_default_controller_error(
                            &ctx,
                            stream_2,
                            controller_2,
                            reader_2,
                            reason,
                        )?;
                        // If canceled1 is false or canceled2 is false, resolve cancelPromise with undefined.
                        if reason_1.get().is_none() || reason_2.get().is_none() {
                            let () = resolve_cancel_promise.call((Value::new_undefined(ctx),))?;
                        }

                        Ok(())
                    },
                }
            },
        )?;

        Ok((branch_1, branch_2))
    }

    // Let pullAlgorithm be the following steps:
    fn readable_stream_default_pull_algorithm(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: OwnedBorrowMut<'js, ReadableStreamDefaultController<'js>>,
        clone_for_branch_2: bool,
        reader: OwnedBorrowMut<'js, ReadableStreamDefaultReader<'js>>,
        reading: Rc<AtomicBool>,
        read_again: Rc<AtomicBool>,
        reason_1: Rc<OnceCell<Value<'js>>>,
        reason_2: Rc<OnceCell<Value<'js>>>,
        branch_1: Rc<OnceCell<Class<'js, ReadableStream<'js>>>>,
        branch_2: Rc<OnceCell<Class<'js, ReadableStream<'js>>>>,
        resolve_cancel_promise: Function<'js>,
        reject_cancel_promise: Function<'js>,
    ) -> Result<Promise<'js>> {
        // If reading is true,
        if reading.load(Ordering::Relaxed) {
            // Set readAgain to true.
            read_again.store(true, Ordering::Relaxed);

            // Return a promise resolved with undefined.
            return promise_resolved_with(&ctx, Ok(Value::new_undefined(ctx.clone())));
        }

        // Set reading to true.
        reading.store(true, Ordering::Relaxed);

        // Let readRequest be a read request with the following items:
        let read_request = ReadableStreamReadRequest {
            chunk_steps: {
                let reading = reading.clone();
                let read_again = read_again.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let branch_1 = branch_1.clone();
                let branch_2 = branch_2.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                let reject_cancel_promise = reject_cancel_promise.clone();
                Box::new(move |stream, controller, reader, chunk| {
                    let ctx = chunk.ctx().clone();
                    let reader = match reader {
                        Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(
                            r,
                        )) => r,
                        Some(_) => {
                            panic!("ReadableStream default tee chunk steps called with BYOB reader")
                        },
                        None => {
                            panic!("ReadableStream default tee chunk steps called without reader")
                        },
                    };
                    let controller = match controller {
                        ReadableStreamControllerOwnedBorrowMut::ReadableStreamDefaultController(
                            c,
                        ) => c,
                        ReadableStreamControllerOwnedBorrowMut::ReadableStreamByteController(_) => {
                            panic!("ReadableStream default tee chunk steps called with byte controller")
                        },
                    };
                    let stream_class = stream.into_inner();
                    let controller_class = controller.into_inner();
                    let reader_class = reader.into_inner();
                    // Queue a microtask to perform the following steps:
                    let f = {
                        let ctx = ctx.clone();
                        let stream_class = stream_class.clone();
                        let controller_class = controller_class.clone();
                        let reader_class = reader_class.clone();
                        move || -> Result<()> {
                            // Set readAgain to false.
                            read_again.store(false, Ordering::Relaxed);

                            // Let chunk1 and chunk2 be chunk.
                            let chunk_1 = chunk.clone();
                            let chunk_2 = chunk.clone();

                            // If canceled2 is false and cloneForBranch2 is true,
                            let chunk_2 = if reason_2.get().is_none() && clone_for_branch_2 {
                                // Let cloneResult be StructuredClone(chunk2).
                                let clone_result = structured_clone(&ctx, chunk_2, Opt(None));
                                match clone_result {
                                    // If cloneResult is an abrupt completion,
                                    Err(Error::Exception) => {
                                        let clone_result = ctx.catch();
                                        let mut stream_1 = OwnedBorrowMut::from_class(
                                            branch_1.get().cloned().expect(
                                                "canceled1 set without branch1 being initialised",
                                            ),
                                        );

                                        let controller_1 = match stream_1.controller.clone().expect(
                                            "canceled1 set without branch1 having a controller",
                                        ) {
                                            ReadableStreamController::ReadableStreamDefaultController(c) => {
                                                OwnedBorrowMut::from_class(c.clone())
                                            },
                                            _ => panic!("controller for streams in ReadableStreamDefaultTee must be ReadableStreamDefaultController"),
                                        };

                                        let reader_1 = stream_1.reader_mut();

                                        // Perform ! ReadableStreamDefaultControllerError(branch1.[[controller]], cloneResult.[[Value]]).
                                        ReadableStreamDefaultController::readable_stream_default_controller_error(
                                            &ctx,
                                            stream_1,
                                            controller_1,
                                            reader_1,
                                            clone_result.clone(),
                                        )?;

                                        let mut stream_2 = OwnedBorrowMut::from_class(
                                            branch_2.get().cloned().clone().expect(
                                                "canceled2 set without branch2 being initialised",
                                            ),
                                        );

                                        let controller_2 = match stream_2.controller.clone().expect(
                                        "canceled2 set without branch2 having a controller",
                                    ) {
                                        ReadableStreamController::ReadableStreamDefaultController(c) => {
                                            OwnedBorrowMut::from_class(c.clone())
                                        },
                                        _ => panic!("controller for streams in ReadableStreamDefaultTee must be ReadableStreamDefaultController"),
                                    };

                                        let reader_2 = stream_2.reader_mut();

                                        // Perform ! ReadableStreamDefaultControllerError(branch2.[[controller]], cloneResult.[[Value]]).
                                        ReadableStreamDefaultController::readable_stream_default_controller_error(
                                            &ctx,
                                            stream_2,
                                            controller_2,
                                            reader_2,
                                            clone_result.clone(),
                                        )?;

                                        // Resolve cancelPromise with ! ReadableStreamCancel(stream, cloneResult.[[Value]]).
                                        let (promise, _, _, _) =
                                            ReadableStream::readable_stream_cancel(
                                                ctx,
                                                OwnedBorrowMut::from_class(stream_class),
                                                OwnedBorrowMut::from_class(controller_class).into(),
                                                Some(
                                                    OwnedBorrowMut::from_class(reader_class).into(),
                                                ),
                                                clone_result,
                                            )?;
                                        let () = resolve_cancel_promise.call((promise,))?;
                                        // Return.
                                        return Ok(());
                                    },
                                    Ok(clone_result) => {
                                        // Otherwise, set chunk2 to cloneResult.[[Value]].
                                        clone_result
                                    },
                                    Err(err) => return Err(err),
                                }
                            } else {
                                chunk_2
                            };

                            // If canceled1 is false, perform ! ReadableStreamDefaultControllerEnqueue(branch1.[[controller]], chunk1).
                            if reason_1.get().is_none() {
                                let mut stream_1 = OwnedBorrowMut::from_class(
                                    branch_1
                                        .get()
                                        .cloned()
                                        .expect("canceled1 set without branch1 being initialised"),
                                );

                                let controller_1 = match stream_1.controller.clone().expect(
                                "canceled1 set without branch1 having a controller",
                            ) {
                                ReadableStreamController::ReadableStreamDefaultController(c) => {
                                    OwnedBorrowMut::from_class(c.clone())
                                },
                                _ => panic!("controller for streams in ReadableStreamDefaultTee must be ReadableStreamDefaultController"),
                            };

                                let reader_1 = stream_1.reader_mut();

                                ReadableStreamDefaultController::readable_stream_default_controller_enqueue(ctx.clone(), controller_1, stream_1, reader_1, chunk_1)?
                            }

                            // If canceled2 is false, perform ! ReadableStreamDefaultControllerEnqueue(branch2.[[controller]], chunk2).
                            if reason_2.get().is_none() {
                                let mut stream_2 = OwnedBorrowMut::from_class(
                                    branch_2
                                        .get()
                                        .cloned()
                                        .expect("canceled2 set without branch2 being initialised"),
                                );

                                let controller_2 = match stream_2.controller.clone().expect(
                                "canceled2 set without branch2 having a controller",
                                ) {
                                    ReadableStreamController::ReadableStreamDefaultController(c) => {
                                        OwnedBorrowMut::from_class(c.clone())
                                    },
                                    _ => panic!("controller for streams in ReadableStreamDefaultTee must be ReadableStreamDefaultController"),
                                };

                                let reader_2 = stream_2.reader_mut();

                                ReadableStreamDefaultController::readable_stream_default_controller_enqueue(ctx.clone(), controller_2, stream_2, reader_2, chunk_2)?
                            }

                            // Set reading to false.
                            reading.store(false, Ordering::Relaxed);

                            // If readAgain is true, perform pullAlgorithm.
                            if read_again.load(Ordering::Relaxed) {
                                Self::readable_stream_default_pull_algorithm(
                                    ctx.clone(),
                                    OwnedBorrowMut::from_class(stream_class),
                                    OwnedBorrowMut::from_class(controller_class),
                                    clone_for_branch_2,
                                    OwnedBorrowMut::from_class(reader_class),
                                    reading.clone(),
                                    read_again.clone(),
                                    reason_1.clone(),
                                    reason_2.clone(),
                                    branch_1.clone(),
                                    branch_2.clone(),
                                    resolve_cancel_promise.clone(),
                                    reject_cancel_promise.clone(),
                                )?;
                            }

                            Ok(())
                        }
                    };

                    () = Function::new(ctx, OnceFn::new(f))?.defer(())?;
                    Ok((
                        OwnedBorrowMut::from_class(stream_class),
                        OwnedBorrowMut::from_class(controller_class).into(),
                        Some(OwnedBorrowMut::from_class(reader_class).into()),
                    ))
                })
            },
            close_steps: {
                let reading = reading.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let branch_1 = branch_1.clone();
                let branch_2 = branch_2.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                Box::new(move |ctx, stream, controller, reader| {
                    // Set reading to false.
                    reading.store(false, Ordering::Relaxed);
                    // If canceled1 is false, perform ! ReadableStreamDefaultControllerClose(branch1.[[controller]]).
                    if reason_1.get().is_none() {
                        let stream = OwnedBorrowMut::from_class(
                            branch_1
                                .get()
                                .expect("close called without branch1 being initialised")
                                .clone(),
                        );

                        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(
                            stream
                                .controller
                                .clone()
                                .expect("close called without branch1 having a controller"),
                        );

                        match controller {
                            ReadableStreamControllerOwnedBorrowMut::ReadableStreamDefaultController(c) => {
                                ReadableStreamDefaultController::readable_stream_default_controller_close(ctx.clone(), stream, c)?
                            },
                            _ => panic!("controller for streams in ReadableStreamDefaultTee must be ReadableStreamDefaultController"),
                        }
                    }
                    // If canceled2 is false, perform ! ReadableStreamDefaultControllerClose(branch2.[[controller]]).
                    if reason_2.get().is_none() {
                        let stream = OwnedBorrowMut::from_class(
                            branch_2
                                .get()
                                .expect("close called without branch2 being initialised")
                                .clone(),
                        );

                        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(
                            stream
                                .controller
                                .clone()
                                .expect("close called without branch2 having a controller"),
                        );

                        match controller {
                            ReadableStreamControllerOwnedBorrowMut::ReadableStreamDefaultController(c) => {
                                ReadableStreamDefaultController::readable_stream_default_controller_close(ctx.clone(), stream, c)?
                            },
                            _ => panic!("controller for streams in ReadableStreamDefaultTee must be ReadableStreamDefaultController"),
                        }
                    }
                    // If canceled1 is false or canceled2 is false, resolve cancelPromise with undefined.
                    if reason_1.get().is_none() || reason_2.get().is_none() {
                        resolve_cancel_promise.call((Value::new_undefined(ctx.clone()),))?
                    }
                    Ok((stream, controller, reader))
                })
            },
            error_steps: {
                let reading = reading.clone();
                Box::new(move |stream, controller, reader, _reason| {
                    // Set reading to false.
                    reading.store(false, Ordering::Relaxed);
                    Ok((stream, controller, reader))
                })
            },
            trace: {
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let branch_1 = branch_1.clone();
                let branch_2 = branch_2.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                let reject_cancel_promise = reject_cancel_promise.clone();
                Box::new(move |tracer| {
                    if let Some(r) = reason_1.get() {
                        r.trace(tracer)
                    }
                    if let Some(r) = reason_2.get() {
                        r.trace(tracer)
                    }
                    if let Some(b) = branch_1.get() {
                        b.trace(tracer)
                    }
                    if let Some(b) = branch_2.get() {
                        b.trace(tracer)
                    }
                    resolve_cancel_promise.trace(tracer);
                    reject_cancel_promise.trace(tracer);
                })
            },
        };

        // Perform ! ReadableStreamDefaultReaderRead(reader, readRequest).
        ReadableStreamDefaultReader::readable_stream_default_reader_read(
            &ctx,
            stream,
            controller.into(),
            reader,
            read_request,
        )?;

        // Return a promise resolved with undefined.
        promise_resolved_with(&ctx, Ok(Value::new_undefined(ctx.clone())))
    }

    // Let cancel1Algorithm be the following steps, taking a reason argument:
    fn readable_stream_cancel_1_algorithm(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        reader: ReadableStreamReaderOwnedBorrowMut<'js>,
        reason_1: Rc<OnceCell<Value<'js>>>,
        reason_2: Rc<OnceCell<Value<'js>>>,
        cancel_promise: Promise<'js>,
        resolve_cancel_promise: Function<'js>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        // Set canceled1 to true.
        // Set reason1 to reason.
        reason_1
            .set(reason.clone())
            .expect("First tee stream already has a cancel reason");

        // If canceled2 is true,
        if let Some(reason_2) = reason_2.get().cloned() {
            // Let compositeReason be ! CreateArrayFromList(« reason1, reason2 »).
            let composite_reason = List((reason, reason_2));
            // Let cancelResult be ! ReadableStreamCancel(stream, compositeReason).
            let (cancel_result, _, _, _) = ReadableStream::readable_stream_cancel(
                ctx.clone(),
                stream,
                controller,
                Some(reader),
                composite_reason.into_js(&ctx)?,
            )?;
            // Resolve cancelPromise with cancelResult.
            let () = resolve_cancel_promise.call((cancel_result,))?;
        }

        // Return cancelPromise.
        Ok(cancel_promise)
    }

    // Let cancel2Algorithm be the following steps, taking a reason argument:
    fn readable_stream_cancel_2_algorithm(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        reader: ReadableStreamReaderOwnedBorrowMut<'js>,
        reason_1: Rc<OnceCell<Value<'js>>>,
        reason_2: Rc<OnceCell<Value<'js>>>,
        cancel_promise: Promise<'js>,
        resolve_cancel_promise: Function<'js>,
        reason: Value<'js>,
    ) -> Result<Promise<'js>> {
        // Set canceled2 to true.
        // Set reason2 to reason.
        reason_2
            .set(reason.clone())
            .expect("Second tee stream already has a cancel reason");

        // If canceled1 is true,
        if let Some(reason_1) = reason_1.get().cloned() {
            // Let compositeReason be ! CreateArrayFromList(« reason1, reason2 »).
            let composite_reason = List((reason_1, reason));
            // Let cancelResult be ! ReadableStreamCancel(stream, compositeReason).
            let (cancel_result, _, _, _) = ReadableStream::readable_stream_cancel(
                ctx.clone(),
                stream,
                controller,
                Some(reader),
                composite_reason.into_js(&ctx)?,
            )?;
            // Resolve cancelPromise with cancelResult.
            let () = resolve_cancel_promise.call((cancel_result,))?;
        }

        // Return cancelPromise.
        Ok(cancel_promise)
    }

    fn readable_byte_stream_tee(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, Self>,
        controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
    ) -> Result<(Class<'js, Self>, Class<'js, Self>)> {
        // Let reader be ? AcquireReadableStreamDefaultReader(stream).
        let (stream, reader) =
            ReadableStreamReader::acquire_readable_stream_default_reader(ctx.clone(), stream)?;
        let reader: Rc<RefCell<ReadableStreamReader<'js>>> = Rc::new(RefCell::new(reader.into()));
        // Let reading be false.
        let reading = Rc::new(AtomicBool::new(false));
        // Let readAgainForBranch1 be false.
        let read_again_for_branch_1 = Rc::new(AtomicBool::new(false));
        // Let readAgainForBranch2 be false.
        let read_again_for_branch_2 = Rc::new(AtomicBool::new(false));
        // Let canceled1 be false.
        // Let canceled2 be false.
        // Let reason1 be undefined.
        let reason_1 = Rc::new(OnceCell::new());
        // Let reason2 be undefined.
        let reason_2 = Rc::new(OnceCell::new());
        // Let branch1 be undefined.
        let branch_1: Rc<OnceCell<Class<'js, Self>>> = Rc::new(OnceCell::new());
        // Let branch2 be undefined.
        let branch_2: Rc<OnceCell<Class<'js, Self>>> = Rc::new(OnceCell::new());
        // Let cancelPromise be a new promise.
        let (cancel_promise, resolve_cancel_promise, reject_cancel_promise) = Promise::new(&ctx)?;

        let stream = stream.into_inner();
        let controller = controller.into_inner();

        // Let pull1Algorithm be the following steps:
        let pull_1_algorithm = PullAlgorithm::Function {
            f: Function::new(ctx.clone(), {
                let stream = stream.clone();
                let controller = controller.clone();
                let reader = reader.clone();
                let reading = reading.clone();
                let read_again_for_branch_1 = read_again_for_branch_1.clone();
                let read_again_for_branch_2 = read_again_for_branch_2.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let branch_1 = branch_1.clone();
                let branch_2 = branch_2.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                let reject_cancel_promise = reject_cancel_promise.clone();
                move |ctx: Ctx<'js>,
                      branch_1_controller: OwnedBorrowMut<
                    'js,
                    ReadableByteStreamController<'js>,
                >| {
                    let stream = OwnedBorrowMut::from_class(stream.clone());
                    let controller = OwnedBorrowMut::from_class(controller.clone());
                    Self::readable_byte_stream_pull_1_algorithm(
                        ctx,
                        stream,
                        controller,
                        reader.clone(),
                        reading.clone(),
                        read_again_for_branch_1.clone(),
                        read_again_for_branch_2.clone(),
                        reason_1.clone(),
                        reason_2.clone(),
                        OwnedBorrowMut::from_class(branch_1.get().cloned().expect("ReadableByteStream tee pull1 algorithm called without branch1 being initialised")),
                        branch_1_controller,
                        OwnedBorrowMut::from_class(branch_2.get().cloned().expect("ReadableByteStream tee pull1 algorithm called without branch2 being initialised")),
                        resolve_cancel_promise.clone(),
                        reject_cancel_promise.clone(),
                    )
                }
            })?,
            underlying_source: Null(None),
        };

        // Let pull2Algorithm be the following steps:
        let pull_2_algorithm = PullAlgorithm::Function {
            f: Function::new(ctx.clone(), {
                let stream = stream.clone();
                let controller = controller.clone();
                let reader = reader.clone();
                let reading = reading.clone();
                let read_again_for_branch_1 = read_again_for_branch_1.clone();
                let read_again_for_branch_2 = read_again_for_branch_2.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let branch_1 = branch_1.clone();
                let branch_2 = branch_2.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                let reject_cancel_promise = reject_cancel_promise.clone();
                move |ctx: Ctx<'js>,
                      branch_2_controller: OwnedBorrowMut<
                    'js,
                    ReadableByteStreamController<'js>,
                >| {
                    let stream = OwnedBorrowMut::from_class(stream.clone());
                    let controller = OwnedBorrowMut::from_class(controller.clone());
                    let branch_2 = OwnedBorrowMut::from_class(branch_2.get().cloned().expect("ReadableByteStream tee pull2 algorithm called without branch2 being initialised"));
                    let branch_1 = OwnedBorrowMut::from_class(branch_1.get().cloned().expect("ReadableByteStream tee pull2 algorithm called without branch1 being initialised"));
                    Self::readable_byte_stream_pull_2_algorithm(
                        ctx,
                        stream,
                        controller,
                        reader.clone(),
                        reading.clone(),
                        read_again_for_branch_1.clone(),
                        read_again_for_branch_2.clone(),
                        reason_1.clone(),
                        reason_2.clone(),
                        branch_1,
                        branch_2,
                        branch_2_controller,
                        resolve_cancel_promise.clone(),
                        reject_cancel_promise.clone(),
                    )
                }
            })?,
            underlying_source: Null(None),
        };

        let cancel_algorithm_1 = CancelAlgorithm::Function {
            f: Function::new(ctx.clone(), {
                OnceFn::new({
                    let stream = stream.clone();
                    let controller = controller.clone();
                    let reader = reader.clone();
                    let reason_1 = reason_1.clone();
                    let reason_2 = reason_2.clone();
                    let cancel_promise = cancel_promise.clone();
                    let resolve_cancel_promise = resolve_cancel_promise.clone();
                    move |reason: Value<'js>| {
                        let stream = OwnedBorrowMut::from_class(stream);
                        let controller = OwnedBorrowMut::from_class(controller);
                        let reader =
                            ReadableStreamReaderOwnedBorrowMut::from_class(reader.borrow().clone());
                        Self::readable_stream_cancel_1_algorithm(
                            reason.ctx().clone(),
                            stream,
                            controller.into(),
                            reader,
                            reason_1,
                            reason_2,
                            cancel_promise,
                            resolve_cancel_promise,
                            reason,
                        )
                    }
                })
            })?,
            underlying_source: Null(None),
        };

        let cancel_algorithm_2 = CancelAlgorithm::Function {
            f: Function::new(ctx.clone(), {
                OnceFn::new({
                    let stream = stream.clone();
                    let controller = controller.clone();
                    let reader = reader.clone();
                    let reason_1 = reason_1.clone();
                    let reason_2 = reason_2.clone();
                    let resolve_cancel_promise = resolve_cancel_promise.clone();
                    move |reason: Value<'js>| {
                        let stream = OwnedBorrowMut::from_class(stream);
                        let controller = OwnedBorrowMut::from_class(controller);
                        let reader =
                            ReadableStreamReaderOwnedBorrowMut::from_class(reader.borrow().clone());
                        Self::readable_stream_cancel_2_algorithm(
                            reason.ctx().clone(),
                            stream,
                            controller.into(),
                            reader,
                            reason_1,
                            reason_2,
                            cancel_promise,
                            resolve_cancel_promise,
                            reason,
                        )
                    }
                })
            })?,
            underlying_source: Null(None),
        };

        // Let startAlgorithm be an algorithm that returns undefined.
        let start_algorithm = StartAlgorithm::ReturnUndefined;

        // Set branch1 to ! CreateReadableByteStream(startAlgorithm, pull1Algorithm, cancel1Algorithm).
        let (branch_1, branch_1_controller) = {
            let (s, c) = Self::create_readable_byte_stream(
                ctx.clone(),
                start_algorithm.clone(),
                pull_1_algorithm.clone(),
                cancel_algorithm_1,
            )?;
            _ = branch_1.set(s.clone());
            (s, c)
        };

        // Set branch2 to ! CreateReadableByteStream(startAlgorithm, pull2Algorithm, cancel2Algorithm).
        let (branch_2, branch_2_controller) = {
            let (s, c) = Self::create_readable_byte_stream(
                ctx.clone(),
                start_algorithm,
                pull_2_algorithm,
                cancel_algorithm_2,
            )?;
            _ = branch_2.set(s.clone());
            (s, c)
        };

        // Perform forwardReaderError, given reader.
        let this_reader = reader.borrow().clone();
        Self::readable_byte_stream_forward_reader_error(
            ctx,
            reader,
            branch_1.clone(),
            branch_1_controller,
            branch_2.clone(),
            branch_2_controller,
            reason_1,
            reason_2,
            this_reader,
            resolve_cancel_promise,
        )?;

        // Return « branch1, branch2 ».
        Ok((branch_1, branch_2))
    }

    // Let forwardReaderError be the following steps, taking a thisReader argument:
    fn readable_byte_stream_forward_reader_error(
        ctx: Ctx<'js>,
        reader: Rc<RefCell<ReadableStreamReader<'js>>>,
        branch_1: Class<'js, Self>,
        branch_1_controller: Class<'js, ReadableByteStreamController<'js>>,
        branch_2: Class<'js, Self>,
        branch_2_controller: Class<'js, ReadableByteStreamController<'js>>,
        reason_1: Rc<OnceCell<Value<'js>>>,
        reason_2: Rc<OnceCell<Value<'js>>>,
        this_reader: ReadableStreamReader<'js>,
        resolve_cancel_promise: Function<'js>,
    ) -> Result<()> {
        // Upon rejection of thisReader.[[closedPromise]] with reason r,
        upon_promise(
            ctx,
            this_reader.closed_promise(),
            move |ctx, result| match result {
                Err(r) => {
                    // If thisReader is not reader, return.
                    if !reader.borrow().eq(&this_reader) {
                        return Ok(());
                    }

                    let mut branch_1 = OwnedBorrowMut::from_class(branch_1);
                    let branch_1_controller = OwnedBorrowMut::from_class(branch_1_controller);

                    let branch_1_reader = branch_1.reader_mut();

                    // Perform ! ReadableByteStreamControllerError(branch1.[[controller]], r).
                    ReadableByteStreamController::readable_byte_stream_controller_error(
                        &ctx,
                        branch_1,
                        branch_1_controller,
                        branch_1_reader,
                        r.clone(),
                    )?;

                    let mut branch_2 = OwnedBorrowMut::from_class(branch_2);
                    let branch_2_controller = OwnedBorrowMut::from_class(branch_2_controller);

                    let branch_2_reader = branch_2.reader_mut();

                    // Perform ! ReadableByteStreamControllerError(branch2.[[controller]], r).
                    ReadableByteStreamController::readable_byte_stream_controller_error(
                        &ctx,
                        branch_2,
                        branch_2_controller,
                        branch_2_reader,
                        r.clone(),
                    )?;

                    // If canceled1 is false or canceled2 is false, resolve cancelPromise with undefined.
                    if reason_1.get().is_none() || reason_2.get().is_none() {
                        let () = resolve_cancel_promise.call((Value::new_undefined(ctx),))?;
                    }
                    Ok(())
                },
                Ok(()) => Ok(()),
            },
        )?;
        Ok(())
    }

    // Let pullWithDefaultReader be the following steps:
    fn readable_byte_stream_pull_with_default_reader(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        reader: Rc<RefCell<ReadableStreamReader<'js>>>,
        reading: Rc<AtomicBool>,
        read_again_for_branch_1: Rc<AtomicBool>,
        read_again_for_branch_2: Rc<AtomicBool>,
        reason_1: Rc<OnceCell<Value<'js>>>,
        reason_2: Rc<OnceCell<Value<'js>>>,
        branch_1: OwnedBorrowMut<'js, ReadableStream<'js>>,
        branch_2: OwnedBorrowMut<'js, ReadableStream<'js>>,
        resolve_cancel_promise: Function<'js>,
        reject_cancel_promise: Function<'js>,
    ) -> Result<()> {
        let branch_1_controller = match branch_1
            .controller
            .clone()
            .expect("pull called without branch1 having a controller")
        {
            ReadableStreamController::ReadableStreamByteController(c) => c,
            _ => panic!("controller for streams in ReadableByteStreamTee must be ReadableByteStreamController"),
        };

        let branch_2_controller = match branch_2
            .controller
            .clone()
            .expect("pull called without branch2 having a controller")
        {
            ReadableStreamController::ReadableStreamByteController(c) => c,
            _ => panic!("controller for streams in ReadableByteStreamTee must be ReadableByteStreamController"),
        };

        let branch_1 = branch_1.into_inner();
        let branch_2 = branch_2.into_inner();

        // If reader implements ReadableStreamBYOBReader,
        let current_reader = reader.borrow().clone();
        let current_reader = match current_reader {
            ReadableStreamReader::ReadableStreamBYOBReader(r) => {
                let byob_reader = OwnedBorrowMut::from_class(r.clone());

                // Perform ! ReadableStreamBYOBReaderRelease(reader).
                (stream, controller) =
                    ReadableStreamBYOBReader::readable_stream_byob_reader_release(
                        &ctx,
                        stream,
                        controller,
                        byob_reader,
                    )?;
                // Set reader to ! AcquireReadableStreamDefaultReader(stream).
                let (s, new_reader) = ReadableStreamReader::acquire_readable_stream_default_reader(
                    ctx.clone(),
                    stream,
                )?;
                stream = s;
                reader.replace(new_reader.clone().into());

                // Perform forwardReaderError, given reader.
                Self::readable_byte_stream_forward_reader_error(
                    ctx.clone(),
                    reader.clone(),
                    branch_1.clone(),
                    branch_1_controller.clone(),
                    branch_2.clone(),
                    branch_2_controller.clone(),
                    reason_1.clone(),
                    reason_2.clone(),
                    new_reader.clone().into(),
                    resolve_cancel_promise.clone(),
                )?;
                new_reader
            },
            ReadableStreamReader::ReadableStreamDefaultReader(r) => r,
        };

        // Let readRequest be a read request with the following items:
        let read_request = ReadableStreamReadRequest {
            chunk_steps: {
                let reader = reader.clone();
                let reading = reading.clone();
                let read_again_for_branch_1 = read_again_for_branch_1.clone();
                let read_again_for_branch_2 = read_again_for_branch_2.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let branch_1 = branch_1.clone();
                let branch_1_controller = branch_1_controller.clone();
                let branch_2 = branch_2.clone();
                let branch_2_controller = branch_2_controller.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                let reject_cancel_promise = reject_cancel_promise.clone();
                Box::new(move |stream, controller, chunk_reader, chunk| {
                    let ctx = chunk.ctx().clone();

                    let controller = match controller {
                        ReadableStreamControllerOwnedBorrowMut::ReadableStreamByteController(c) => {
                            c
                        },
                        ReadableStreamControllerOwnedBorrowMut::ReadableStreamDefaultController(
                            _,
                        ) => {
                            panic!(
                                "ReadableByteStream tee chunk steps called with default controller"
                            )
                        },
                    };
                    let stream_class = stream.into_inner();
                    let controller_class = controller.into_inner();
                    let chunk_reader_class = chunk_reader.map(|r| r.into_inner());
                    let chunk = ViewBytes::from_js(&ctx, chunk)?;
                    // Queue a microtask to perform the following steps:
                    let f = {
                        let ctx = ctx.clone();
                        let stream_class = stream_class.clone();
                        let controller_class = controller_class.clone();
                        let chunk_reader_class = chunk_reader_class.clone();
                        move || -> Result<()> {
                            // Set readAgainForBranch1 to false.
                            read_again_for_branch_1.store(false, Ordering::Relaxed);
                            // Set readAgainForBranch2 to false.
                            read_again_for_branch_2.store(false, Ordering::Relaxed);

                            // Let chunk1 and chunk2 be chunk.
                            let chunk_1 = chunk.clone();
                            let mut chunk_2 = chunk.clone();

                            // If canceled1 is false and canceled2 is false,
                            if reason_1.get().is_none() && reason_2.get().is_none() {
                                // Let cloneResult be CloneAsUint8Array(chunk).
                                match clone_as_uint8_array(ctx.clone(), chunk) {
                                    // If cloneResult is an abrupt completion,
                                    Err(Error::Exception) => {
                                        let err = ctx.catch();

                                        let mut branch_1 = OwnedBorrowMut::from_class(branch_1);
                                        let branch_1_controller =
                                            OwnedBorrowMut::from_class(branch_1_controller);
                                        let branch_1_reader = branch_1.reader_mut();

                                        // Perform ! ReadableByteStreamControllerError(branch1.[[controller]], cloneResult.[[Value]]).
                                        ReadableByteStreamController::readable_byte_stream_controller_error(
                                            &ctx,
                                            branch_1,
                                            branch_1_controller,
                                            branch_1_reader,
                                            err.clone(),
                                        )?;

                                        let mut branch_2 = OwnedBorrowMut::from_class(branch_2);
                                        let branch_2_controller =
                                            OwnedBorrowMut::from_class(branch_2_controller);
                                        let branch_2_reader = branch_2.reader_mut();

                                        // Perform ! ReadableByteStreamControllerError(branch2.[[controller]], cloneResult.[[Value]]).
                                        ReadableByteStreamController::readable_byte_stream_controller_error(
                                            &ctx,
                                            branch_2,
                                            branch_2_controller,
                                            branch_2_reader,
                                            err.clone(),
                                        )?;

                                        // Resolve cancelPromise with ! ReadableStreamCancel(stream, cloneResult.[[Value]]).
                                        let (promise, _, _, _) =
                                            ReadableStream::readable_stream_cancel(
                                                ctx,
                                                OwnedBorrowMut::from_class(stream_class),
                                                OwnedBorrowMut::from_class(controller_class).into(),
                                                chunk_reader_class.map(
                                                    ReadableStreamReaderOwnedBorrowMut::from_class,
                                                ),
                                                err.clone(),
                                            )?;
                                        let () = resolve_cancel_promise.call((promise,))?;

                                        // Return.
                                        return Ok(());
                                    },
                                    // Otherwise, set chunk2 to cloneResult.[[Value]].
                                    Ok(clone_result) => chunk_2 = clone_result,
                                    Err(err) => return Err(err),
                                };
                            }

                            // If canceled1 is false, perform ! ReadableByteStreamControllerEnqueue(branch1.[[controller]], chunk1).
                            if reason_1.get().is_none() {
                                let mut branch_1 = OwnedBorrowMut::from_class(branch_1.clone());
                                let branch_1_controller =
                                    OwnedBorrowMut::from_class(branch_1_controller.clone());
                                let branch_1_reader = branch_1.reader_mut();
                                ReadableByteStreamController::readable_byte_stream_controller_enqueue(&ctx, branch_1_controller, branch_1, branch_1_reader, chunk_1)?;
                            }

                            // If canceled2 is false, perform ! ReadableByteStreamControllerEnqueue(branch2.[[controller]], chunk2).
                            if reason_2.get().is_none() {
                                let mut branch_2 = OwnedBorrowMut::from_class(branch_2.clone());
                                let branch_2_controller =
                                    OwnedBorrowMut::from_class(branch_2_controller.clone());
                                let branch_2_reader = branch_2.reader_mut();
                                ReadableByteStreamController::readable_byte_stream_controller_enqueue(&ctx, branch_2_controller, branch_2, branch_2_reader, chunk_2)?;
                            }

                            // Set reading to false.
                            reading.store(false, Ordering::Relaxed);

                            let branch_1 = OwnedBorrowMut::from_class(branch_1);
                            let branch_2 = OwnedBorrowMut::from_class(branch_2);

                            // If readAgainForBranch1 is true, perform pull1Algorithm.
                            if read_again_for_branch_1.load(Ordering::Relaxed) {
                                let branch_1_controller =
                                    OwnedBorrowMut::from_class(branch_1_controller);
                                Self::readable_byte_stream_pull_1_algorithm(
                                    ctx.clone(),
                                    OwnedBorrowMut::from_class(stream_class),
                                    OwnedBorrowMut::from_class(controller_class),
                                    reader,
                                    reading.clone(),
                                    read_again_for_branch_1.clone(),
                                    read_again_for_branch_2.clone(),
                                    reason_1.clone(),
                                    reason_2.clone(),
                                    branch_1,
                                    branch_1_controller,
                                    branch_2,
                                    resolve_cancel_promise.clone(),
                                    reject_cancel_promise.clone(),
                                )?;
                            } else if read_again_for_branch_2.load(Ordering::Relaxed) {
                                let branch_2_controller =
                                    OwnedBorrowMut::from_class(branch_2_controller);
                                // Otherwise, if readAgainForBranch2 is true, perform pull2Algorithm.
                                Self::readable_byte_stream_pull_2_algorithm(
                                    ctx.clone(),
                                    OwnedBorrowMut::from_class(stream_class),
                                    OwnedBorrowMut::from_class(controller_class),
                                    reader,
                                    reading.clone(),
                                    read_again_for_branch_1.clone(),
                                    read_again_for_branch_2.clone(),
                                    reason_1.clone(),
                                    reason_2.clone(),
                                    branch_1,
                                    branch_2,
                                    branch_2_controller,
                                    resolve_cancel_promise.clone(),
                                    reject_cancel_promise.clone(),
                                )?;
                            }

                            Ok(())
                        }
                    };

                    () = Function::new(ctx, OnceFn::new(f))?.defer(())?;
                    Ok((
                        OwnedBorrowMut::from_class(stream_class),
                        OwnedBorrowMut::from_class(controller_class).into(),
                        chunk_reader_class.map(ReadableStreamReaderOwnedBorrowMut::from_class),
                    ))
                })
            },
            close_steps: {
                let reading = reading.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let branch_1 = branch_1.clone();
                let branch_1_controller = branch_1_controller.clone();
                let branch_2 = branch_2.clone();
                let branch_2_controller = branch_2_controller.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                Box::new(move |ctx, stream, controller, reader| {
                    // Set reading to false.
                    reading.store(false, Ordering::Relaxed);

                    let mut branch_1 = OwnedBorrowMut::from_class(branch_1);
                    let mut branch_1_controller = OwnedBorrowMut::from_class(branch_1_controller);
                    let mut branch_1_reader = branch_1.reader_mut();

                    let mut branch_2 = OwnedBorrowMut::from_class(branch_2);
                    let mut branch_2_controller = OwnedBorrowMut::from_class(branch_2_controller);
                    let mut branch_2_reader = branch_2.reader_mut();

                    // If canceled1 is false, perform ! ReadableByteStreamControllerClose(branch1.[[controller]]).
                    if reason_1.get().is_none() {
                        (branch_1, branch_1_controller, branch_1_reader) =
                            ReadableByteStreamController::readable_byte_stream_controller_close(
                                ctx.clone(),
                                branch_1,
                                branch_1_controller,
                                branch_1_reader,
                            )?;
                    }
                    // If canceled2 is false, perform ! ReadableByteStreamControllerClose(branch2.[[controller]]).
                    if reason_2.get().is_none() {
                        (branch_2, branch_2_controller, branch_2_reader) =
                            ReadableByteStreamController::readable_byte_stream_controller_close(
                                ctx.clone(),
                                branch_2,
                                branch_2_controller,
                                branch_2_reader,
                            )?;
                    }
                    // If branch1.[[controller]].[[pendingPullIntos]] is not empty, perform ! ReadableByteStreamControllerRespond(branch1.[[controller]], 0).
                    if !branch_1_controller.pending_pull_intos.is_empty() {
                        ReadableByteStreamController::readable_byte_stream_controller_respond(
                            ctx.clone(),
                            branch_1,
                            branch_1_controller,
                            branch_1_reader,
                            0,
                        )?
                    }

                    // If branch2.[[controller]].[[pendingPullIntos]] is not empty, perform ! ReadableByteStreamControllerRespond(branch2.[[controller]], 0).
                    if !branch_2_controller.pending_pull_intos.is_empty() {
                        ReadableByteStreamController::readable_byte_stream_controller_respond(
                            ctx.clone(),
                            branch_2,
                            branch_2_controller,
                            branch_2_reader,
                            0,
                        )?
                    }

                    // If canceled1 is false or canceled2 is false, resolve cancelPromise with undefined.
                    if reason_1.get().is_none() || reason_2.get().is_none() {
                        resolve_cancel_promise.call((Value::new_undefined(ctx.clone()),))?
                    }
                    Ok((stream, controller, reader))
                })
            },
            error_steps: {
                let reading = reading.clone();
                Box::new(move |stream, controller, reader, _reason| {
                    // Set reading to false.
                    reading.store(false, Ordering::Relaxed);
                    Ok((stream, controller, reader))
                })
            },
            trace: {
                let reader = reader.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let branch_1 = branch_1.clone();
                let branch_1_controller = branch_1_controller.clone();
                let branch_2 = branch_2.clone();
                let branch_2_controller = branch_2_controller.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                let reject_cancel_promise = reject_cancel_promise.clone();
                Box::new(move |tracer| {
                    if let Ok(r) = reader.try_borrow() {
                        r.trace(tracer)
                    }
                    if let Some(r) = reason_1.get() {
                        r.trace(tracer)
                    }
                    if let Some(r) = reason_2.get() {
                        r.trace(tracer)
                    }
                    branch_1.trace(tracer);
                    branch_1_controller.trace(tracer);
                    branch_2.trace(tracer);
                    branch_2_controller.trace(tracer);
                    resolve_cancel_promise.trace(tracer);
                    reject_cancel_promise.trace(tracer);
                })
            },
        };

        // Perform ! ReadableStreamDefaultReaderRead(reader, readRequest).
        ReadableStreamDefaultReader::readable_stream_default_reader_read(
            &ctx,
            stream,
            controller.into(),
            OwnedBorrowMut::from_class(current_reader),
            read_request,
        )
    }

    fn readable_byte_stream_pull_with_byob_reader(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        reader: Rc<RefCell<ReadableStreamReader<'js>>>,
        reading: Rc<AtomicBool>,
        read_again_for_branch_1: Rc<AtomicBool>,
        read_again_for_branch_2: Rc<AtomicBool>,
        reason_1: Rc<OnceCell<Value<'js>>>,
        reason_2: Rc<OnceCell<Value<'js>>>,
        branch_1: OwnedBorrowMut<'js, ReadableStream<'js>>,
        branch_2: OwnedBorrowMut<'js, ReadableStream<'js>>,
        resolve_cancel_promise: Function<'js>,
        reject_cancel_promise: Function<'js>,
        view: ViewBytes<'js>,
        for_branch_2: bool,
    ) -> Result<()> {
        let branch_1_controller = match branch_1
            .controller
            .clone()
            .expect("pull called without branch1 having a controller")
        {
            ReadableStreamController::ReadableStreamByteController(c) => c,
            _ => panic!("controller for streams in ReadableByteStreamTee must be ReadableByteStreamController"),
        };

        let branch_2_controller = match branch_2
            .controller
            .clone()
            .expect("pull called without branch2 having a controller")
        {
            ReadableStreamController::ReadableStreamByteController(c) => c,
            _ => panic!("controller for streams in ReadableByteStreamTee must be ReadableByteStreamController"),
        };

        let branch_1 = branch_1.into_inner();
        let branch_2 = branch_2.into_inner();

        // If reader implements ReadableStreamDefaultReader,
        let current_reader = reader.borrow().clone();
        let current_reader = match current_reader {
            ReadableStreamReader::ReadableStreamDefaultReader(r) => {
                let default_reader = OwnedBorrowMut::from_class(r.clone());

                // Perform ! ReadableStreamDefaultReaderRelease(reader).
                let mut c = controller.into();
                (stream, c, _) =
                    ReadableStreamDefaultReader::readable_stream_default_reader_release(
                        &ctx,
                        stream,
                        c,
                        default_reader,
                    )?;
                controller = c.into_byte_controller().expect("readable_stream_default_reader_release must return the same type of controller");

                // Set reader to ! AcquireReadableStreamBYOBReader(stream).
                let (s, new_reader) =
                    ReadableStreamReader::acquire_readable_stream_byob_reader(ctx.clone(), stream)?;
                stream = s;
                reader.replace(new_reader.clone().into());

                // Perform forwardReaderError, given reader.
                Self::readable_byte_stream_forward_reader_error(
                    ctx.clone(),
                    reader.clone(),
                    branch_1.clone(),
                    branch_1_controller.clone(),
                    branch_2.clone(),
                    branch_2_controller.clone(),
                    reason_1.clone(),
                    reason_2.clone(),
                    new_reader.clone().into(),
                    resolve_cancel_promise.clone(),
                )?;

                new_reader
            },
            ReadableStreamReader::ReadableStreamBYOBReader(r) => r.clone(),
        };

        // Let byobBranch be branch2 if forBranch2 is true, and branch1 otherwise.
        // Let otherBranch be branch2 if forBranch2 is false, and branch1 otherwise.
        let (byob_branch, byob_branch_controller, other_branch, other_branch_controller) =
            if for_branch_2 {
                (
                    branch_2.clone(),
                    branch_2_controller.clone(),
                    branch_1.clone(),
                    branch_1_controller.clone(),
                )
            } else {
                (
                    branch_1.clone(),
                    branch_1_controller.clone(),
                    branch_2.clone(),
                    branch_2_controller.clone(),
                )
            };

        // Let readIntoRequest be a read-into request with the following items:
        let read_into_request = ReadableStreamReadIntoRequest {
            chunk_steps: {
                let reader = reader.clone();
                let reading = reading.clone();
                let read_again_for_branch_1 = read_again_for_branch_1.clone();
                let read_again_for_branch_2 = read_again_for_branch_2.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let byob_branch = byob_branch.clone();
                let byob_branch_controller = byob_branch_controller.clone();
                let other_branch = other_branch.clone();
                let other_branch_controller = other_branch_controller.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                let reject_cancel_promise = reject_cancel_promise.clone();
                Box::new(move |stream, controller, chunk_reader, chunk| {
                    let ctx = chunk.ctx().clone();

                    let stream_class = stream.into_inner();
                    let controller_class = controller.into_inner();
                    let chunk_reader_class = chunk_reader.into_inner();

                    let chunk = ViewBytes::from_js(&ctx, chunk)?;
                    // Queue a microtask to perform the following steps:
                    let f = {
                        let ctx = ctx.clone();
                        let stream_class = stream_class.clone();
                        let controller_class = controller_class.clone();
                        let chunk_reader_class = chunk_reader_class.clone();
                        move || -> Result<()> {
                            // Set readAgainForBranch1 to false.
                            read_again_for_branch_1.store(false, Ordering::Relaxed);
                            // Set readAgainForBranch2 to false.
                            read_again_for_branch_2.store(false, Ordering::Relaxed);

                            // Let byobCanceled be canceled2 if forBranch2 is true, and canceled1 otherwise.
                            // Let otherCanceled be canceled2 if forBranch2 is false, and canceled1 otherwise.
                            let (byob_canceled, other_canceled) = if for_branch_2 {
                                (reason_2.get().is_some(), reason_1.get().is_some())
                            } else {
                                (reason_1.get().is_some(), reason_2.get().is_some())
                            };

                            // If otherCanceled is false,
                            if !other_canceled {
                                // Let cloneResult be CloneAsUint8Array(chunk).
                                match clone_as_uint8_array(ctx.clone(), chunk.clone()) {
                                    // If cloneResult is an abrupt completion,
                                    Err(Error::Exception) => {
                                        let err = ctx.catch();

                                        let mut byob_branch =
                                            OwnedBorrowMut::from_class(byob_branch);
                                        let byob_branch_controller =
                                            OwnedBorrowMut::from_class(byob_branch_controller);
                                        let byob_branch_reader = byob_branch.reader_mut();

                                        // Perform ! ReadableByteStreamControllerError(byobBranch.[[controller]], cloneResult.[[Value]]).
                                        ReadableByteStreamController::readable_byte_stream_controller_error(
                                            &ctx,
                                            byob_branch,
                                            byob_branch_controller,
                                            byob_branch_reader,
                                            err.clone(),
                                        )?;

                                        let mut other_branch =
                                            OwnedBorrowMut::from_class(other_branch);
                                        let other_branch_controller =
                                            OwnedBorrowMut::from_class(other_branch_controller);
                                        let other_branch_reader = other_branch.reader_mut();

                                        // Perform ! ReadableByteStreamControllerError(otherBranch.[[controller]], cloneResult.[[Value]]).
                                        ReadableByteStreamController::readable_byte_stream_controller_error(
                                            &ctx,
                                            other_branch,
                                            other_branch_controller,
                                            other_branch_reader,
                                            err.clone(),
                                        )?;

                                        // Resolve cancelPromise with ! ReadableStreamCancel(stream, cloneResult.[[Value]]).
                                        let (promise, _, _, _) =
                                            ReadableStream::readable_stream_cancel(
                                                ctx,
                                                OwnedBorrowMut::from_class(stream_class),
                                                OwnedBorrowMut::from_class(controller_class).into(),
                                                Some(
                                                    OwnedBorrowMut::from_class(chunk_reader_class)
                                                        .into(),
                                                ),
                                                err.clone(),
                                            )?;
                                        let () = resolve_cancel_promise.call((promise,))?;

                                        // Return.
                                        return Ok(());
                                    },
                                    // Otherwise, let clonedChunk be cloneResult.[[Value]].
                                    Ok(cloned_chunk) => {
                                        // If byobCanceled is false, perform ! ReadableByteStreamControllerRespondWithNewView(byobBranch.[[controller]], chunk).
                                        if !byob_canceled {
                                            let mut byob_branch =
                                                OwnedBorrowMut::from_class(byob_branch);
                                            let byob_branch_controller =
                                                OwnedBorrowMut::from_class(byob_branch_controller);
                                            let byob_branch_reader = byob_branch.reader_mut();

                                            ReadableByteStreamController::readable_byte_stream_controller_respond_with_new_view(ctx.clone(), byob_branch, byob_branch_controller, byob_branch_reader, chunk)?;
                                        }

                                        let mut other_branch =
                                            OwnedBorrowMut::from_class(other_branch);
                                        let other_branch_controller =
                                            OwnedBorrowMut::from_class(other_branch_controller);
                                        let other_branch_reader = other_branch.reader_mut();

                                        // Perform ! ReadableByteStreamControllerEnqueue(otherBranch.[[controller]], clonedChunk).
                                        ReadableByteStreamController::readable_byte_stream_controller_enqueue(&ctx, other_branch_controller, other_branch, other_branch_reader, cloned_chunk)?;
                                    },
                                    Err(err) => return Err(err),
                                };
                            } else if !byob_canceled {
                                let mut byob_branch = OwnedBorrowMut::from_class(byob_branch);
                                let byob_branch_controller =
                                    OwnedBorrowMut::from_class(byob_branch_controller);
                                let byob_branch_reader = byob_branch.reader_mut();

                                // Otherwise, if byobCanceled is false, perform ! ReadableByteStreamControllerRespondWithNewView(byobBranch.[[controller]], chunk).
                                ReadableByteStreamController::readable_byte_stream_controller_respond_with_new_view(ctx.clone(), byob_branch, byob_branch_controller, byob_branch_reader, chunk)?;
                            }

                            let branch_1 = OwnedBorrowMut::from_class(branch_1);
                            let branch_2 = OwnedBorrowMut::from_class(branch_2);

                            // Set reading to false.
                            reading.store(false, Ordering::Relaxed);

                            // If readAgainForBranch1 is true, perform pull1Algorithm.
                            if read_again_for_branch_1.load(Ordering::Relaxed) {
                                let branch_1_controller =
                                    OwnedBorrowMut::from_class(branch_1_controller);

                                Self::readable_byte_stream_pull_1_algorithm(
                                    ctx.clone(),
                                    OwnedBorrowMut::from_class(stream_class),
                                    OwnedBorrowMut::from_class(controller_class),
                                    reader,
                                    reading.clone(),
                                    read_again_for_branch_1.clone(),
                                    read_again_for_branch_2.clone(),
                                    reason_1.clone(),
                                    reason_2.clone(),
                                    branch_1,
                                    branch_1_controller,
                                    branch_2,
                                    resolve_cancel_promise.clone(),
                                    reject_cancel_promise.clone(),
                                )?;
                            } else if read_again_for_branch_2.load(Ordering::Relaxed) {
                                let branch_2_controller =
                                    OwnedBorrowMut::from_class(branch_2_controller);

                                // Otherwise, if readAgainForBranch2 is true, perform pull2Algorithm.
                                Self::readable_byte_stream_pull_2_algorithm(
                                    ctx.clone(),
                                    OwnedBorrowMut::from_class(stream_class),
                                    OwnedBorrowMut::from_class(controller_class),
                                    reader,
                                    reading.clone(),
                                    read_again_for_branch_1.clone(),
                                    read_again_for_branch_2.clone(),
                                    reason_1.clone(),
                                    reason_2.clone(),
                                    branch_1,
                                    branch_2,
                                    branch_2_controller,
                                    resolve_cancel_promise.clone(),
                                    reject_cancel_promise.clone(),
                                )?;
                            }

                            Ok(())
                        }
                    };

                    () = Function::new(ctx, OnceFn::new(f))?.defer(())?;

                    Ok((
                        OwnedBorrowMut::from_class(stream_class),
                        OwnedBorrowMut::from_class(controller_class),
                        OwnedBorrowMut::from_class(chunk_reader_class),
                    ))
                })
            },
            close_steps: {
                let reading = reading.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let byob_branch = byob_branch.clone();
                let byob_branch_controller = byob_branch_controller.clone();
                let other_branch = other_branch.clone();
                let other_branch_controller = other_branch_controller.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                Box::new(move |stream, controller, reader, chunk| {
                    let ctx = chunk.ctx().clone();

                    // Set reading to false.
                    reading.store(false, Ordering::Relaxed);

                    // Let byobCanceled be canceled2 if forBranch2 is true, and canceled1 otherwise.
                    // Let otherCanceled be canceled2 if forBranch2 is false, and canceled1 otherwise.
                    let (byob_canceled, other_canceled) = if for_branch_2 {
                        (reason_2.get().is_some(), reason_1.get().is_some())
                    } else {
                        (reason_1.get().is_some(), reason_2.get().is_some())
                    };

                    // If byobCanceled is false, perform ! ReadableByteStreamControllerClose(byobBranch.[[controller]]).
                    if !byob_canceled {
                        let mut byob_branch = OwnedBorrowMut::from_class(byob_branch.clone());
                        let byob_branch_controller =
                            OwnedBorrowMut::from_class(byob_branch_controller.clone());
                        let byob_branch_reader = byob_branch.reader_mut();

                        ReadableByteStreamController::readable_byte_stream_controller_close(
                            ctx.clone(),
                            byob_branch,
                            byob_branch_controller,
                            byob_branch_reader,
                        )?;
                    }
                    // If otherCanceled is false, perform ! ReadableByteStreamControllerClose(otherBranch.[[controller]]).
                    if !other_canceled {
                        let mut other_branch = OwnedBorrowMut::from_class(other_branch.clone());
                        let other_branch_controller =
                            OwnedBorrowMut::from_class(other_branch_controller.clone());
                        let other_branch_reader = other_branch.reader_mut();

                        ReadableByteStreamController::readable_byte_stream_controller_close(
                            ctx.clone(),
                            other_branch,
                            other_branch_controller,
                            other_branch_reader,
                        )?;
                    }

                    // If chunk is not undefined,
                    if !chunk.is_undefined() {
                        let chunk = ViewBytes::from_js(&ctx, chunk)?;

                        // If byobCanceled is false, perform ! ReadableByteStreamControllerRespondWithNewView(byobBranch.[[controller]], chunk).
                        if !byob_canceled {
                            let mut byob_branch = OwnedBorrowMut::from_class(byob_branch);
                            let byob_branch_controller =
                                OwnedBorrowMut::from_class(byob_branch_controller);
                            let byob_branch_reader = byob_branch.reader_mut();
                            ReadableByteStreamController::readable_byte_stream_controller_respond_with_new_view(ctx.clone(), byob_branch, byob_branch_controller, byob_branch_reader, chunk)?;
                        }

                        let other_branch_controller =
                            OwnedBorrowMut::from_class(other_branch_controller);

                        // If otherCanceled is false and otherBranch.[[controller]].[[pendingPullIntos]] is not empty, perform ! ReadableByteStreamControllerRespond(otherBranch.[[controller]], 0).
                        if !other_canceled && !other_branch_controller.pending_pull_intos.is_empty()
                        {
                            let mut other_branch = OwnedBorrowMut::from_class(other_branch);
                            let other_branch_reader = other_branch.reader_mut();

                            ReadableByteStreamController::readable_byte_stream_controller_respond(
                                ctx.clone(),
                                other_branch,
                                other_branch_controller,
                                other_branch_reader,
                                0,
                            )?;
                        }
                    }

                    // If byobCanceled is false or otherCanceled is false, resolve cancelPromise with undefined.
                    if !byob_canceled || !other_canceled {
                        resolve_cancel_promise.call((Value::new_undefined(ctx.clone()),))?
                    }

                    Ok((stream, controller, reader))
                })
            },
            error_steps: {
                let reading = reading.clone();
                Box::new(move |stream, controller, reader, _reason| {
                    // Set reading to false.
                    reading.store(false, Ordering::Relaxed);
                    Ok((stream, controller, reader))
                })
            },
            trace: {
                let reader = reader.clone();
                let reason_1 = reason_1.clone();
                let reason_2 = reason_2.clone();
                let byob_branch = byob_branch.clone();
                let byob_branch_controller = byob_branch_controller.clone();
                let other_branch = other_branch.clone();
                let other_branch_controller = other_branch_controller.clone();
                let resolve_cancel_promise = resolve_cancel_promise.clone();
                let reject_cancel_promise = reject_cancel_promise.clone();
                Box::new(move |tracer| {
                    if let Ok(r) = reader.try_borrow() {
                        r.trace(tracer)
                    }
                    if let Some(r) = reason_1.get() {
                        r.trace(tracer)
                    }
                    if let Some(r) = reason_2.get() {
                        r.trace(tracer)
                    }
                    byob_branch.trace(tracer);
                    byob_branch_controller.trace(tracer);
                    other_branch.trace(tracer);
                    other_branch_controller.trace(tracer);
                    resolve_cancel_promise.trace(tracer);
                    reject_cancel_promise.trace(tracer);
                })
            },
        };

        // Perform ! ReadableStreamBYOBReaderRead(reader, view, 1, readIntoRequest).
        ReadableStreamBYOBReader::readable_stream_byob_reader_read(
            &ctx,
            stream,
            controller,
            OwnedBorrowMut::from_class(current_reader),
            view,
            1,
            read_into_request,
        )
    }

    // Let pull1Algorithm be the following steps:
    fn readable_byte_stream_pull_1_algorithm(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        reader: Rc<RefCell<ReadableStreamReader<'js>>>,
        reading: Rc<AtomicBool>,
        read_again_for_branch_1: Rc<AtomicBool>,
        read_again_for_branch_2: Rc<AtomicBool>,
        reason_1: Rc<OnceCell<Value<'js>>>,
        reason_2: Rc<OnceCell<Value<'js>>>,
        branch_1: OwnedBorrowMut<'js, ReadableStream<'js>>,
        branch_1_controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        branch_2: OwnedBorrowMut<'js, ReadableStream<'js>>,
        resolve_cancel_promise: Function<'js>,
        reject_cancel_promise: Function<'js>,
    ) -> Result<Promise<'js>> {
        // If reading is true,
        if reading.swap(true, Ordering::Relaxed) {
            // Set readAgainForBranch1 to true.
            read_again_for_branch_1.store(true, Ordering::Relaxed);
            // Return a promise resolved with undefined.
            return promise_resolved_with(&ctx.clone(), Ok(Value::new_undefined(ctx)));
        }
        // Set reading to true.

        // Let byobRequest be ! ReadableByteStreamControllerGetBYOBRequest(branch1.[[controller]]).
        let byob_request =
            ReadableByteStreamController::readable_byte_stream_controller_get_byob_request(
                ctx.clone(),
                branch_1_controller,
            )?;

        // If byobRequest is null, perform pullWithDefaultReader.
        match byob_request.0 {
            None => Self::readable_byte_stream_pull_with_default_reader(
                ctx.clone(),
                stream,
                controller,
                reader.clone(),
                reading.clone(),
                read_again_for_branch_1,
                read_again_for_branch_2,
                reason_1,
                reason_2,
                branch_1,
                branch_2,
                resolve_cancel_promise.clone(),
                reject_cancel_promise.clone(),
            )?,
            // Otherwise, perform pullWithBYOBReader, given byobRequest.[[view]] and false.
            Some(byob_request) => {
                let view = byob_request.borrow().view.clone().expect(
                    "ReadableByteStream tee pull1Algorithm called with invalidated byobRequest",
                );
                Self::readable_byte_stream_pull_with_byob_reader(
                    ctx.clone(),
                    stream,
                    controller,
                    reader.clone(),
                    reading.clone(),
                    read_again_for_branch_1,
                    read_again_for_branch_2,
                    reason_1,
                    reason_2,
                    branch_1,
                    branch_2,
                    resolve_cancel_promise.clone(),
                    reject_cancel_promise.clone(),
                    view,
                    false,
                )?
            },
        }

        // Return a promise resolved with undefined.
        return promise_resolved_with(&ctx.clone(), Ok(Value::new_undefined(ctx)));
    }

    // Let pull2Algorithm be the following steps:
    fn readable_byte_stream_pull_2_algorithm(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        reader: Rc<RefCell<ReadableStreamReader<'js>>>,
        reading: Rc<AtomicBool>,
        read_again_for_branch_1: Rc<AtomicBool>,
        read_again_for_branch_2: Rc<AtomicBool>,
        reason_1: Rc<OnceCell<Value<'js>>>,
        reason_2: Rc<OnceCell<Value<'js>>>,
        branch_1: OwnedBorrowMut<'js, ReadableStream<'js>>,
        branch_2: OwnedBorrowMut<'js, ReadableStream<'js>>,
        branch_2_controller: OwnedBorrowMut<'js, ReadableByteStreamController<'js>>,
        resolve_cancel_promise: Function<'js>,
        reject_cancel_promise: Function<'js>,
    ) -> Result<Promise<'js>> {
        // If reading is true,
        if reading.swap(true, Ordering::Relaxed) {
            // Set readAgainForBranch2 to true.
            read_again_for_branch_2.store(true, Ordering::Relaxed);
            // Return a promise resolved with undefined.
            return promise_resolved_with(&ctx.clone(), Ok(Value::new_undefined(ctx)));
        }
        // Set reading to true.

        // Let byobRequest be ! ReadableByteStreamControllerGetBYOBRequest(branch2.[[controller]]).
        let byob_request =
            ReadableByteStreamController::readable_byte_stream_controller_get_byob_request(
                ctx.clone(),
                branch_2_controller,
            )?;

        // If byobRequest is null, perform pullWithDefaultReader.
        match byob_request.0 {
            None => Self::readable_byte_stream_pull_with_default_reader(
                ctx.clone(),
                stream,
                controller,
                reader.clone(),
                reading.clone(),
                read_again_for_branch_1,
                read_again_for_branch_2,
                reason_1,
                reason_2,
                branch_1,
                branch_2,
                resolve_cancel_promise,
                reject_cancel_promise,
            )?,
            // Otherwise, perform pullWithBYOBReader, given byobRequest.[[view]] and true.
            Some(byob_request) => Self::readable_byte_stream_pull_with_byob_reader(
                ctx.clone(),
                stream,
                controller,
                reader.clone(),
                reading.clone(),
                read_again_for_branch_1,
                read_again_for_branch_2,
                reason_1,
                reason_2,
                branch_1,
                branch_2,
                resolve_cancel_promise,
                reject_cancel_promise,
                byob_request.borrow().view.clone().expect(
                    "ReadableByteStream tee pull2Algorithm called with invalidated byobRequest",
                ),
                true,
            )?,
        }

        // Return a promise resolved with undefined.
        return promise_resolved_with(&ctx.clone(), Ok(Value::new_undefined(ctx)));
    }
}

fn clone_as_uint8_array<'js>(ctx: Ctx<'js>, chunk: ViewBytes<'js>) -> Result<ViewBytes<'js>> {
    let (buffer, byte_length, byte_offset) = chunk.get_array_buffer()?.unwrap();

    // Let buffer be ? CloneArrayBuffer(O.[[ViewedArrayBuffer]], O.[[ByteOffset]], O.[[ByteLength]], %ArrayBuffer%).
    let buffer = ArrayBuffer::new_copy(
        ctx.clone(),
        &buffer
            .as_bytes()
            .expect("CloneAsUInt8Array called on detached buffer")
            [byte_offset..byte_offset + byte_length],
    )?;

    // Let array be ! Construct(%Uint8Array%, « buffer »).
    // Return array.
    let ctor: Constructor = ctx.globals().get(PredefinedAtom::Uint8Array)?;
    ctor.construct((buffer,))
}
