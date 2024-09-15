use std::{
    cell::OnceCell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

use rquickjs::{
    class::{OwnedBorrowMut, Trace},
    prelude::{List, OnceFn, Opt},
    Class, Ctx, Error, Function, IntoJs, Promise, Result, Value,
};

use crate::{
    modules::stream::readable::{
        default_controller::ReadableStreamDefaultController, promise_resolved_with, upon_promise,
        CancelAlgorithm, Null, PullAlgorithm, ReadableStreamDefaultReader,
        ReadableStreamReadRequest, ReadableStreamReader, ReadableStreamReaderOwnedBorrowMut,
        StartAlgorithm,
    },
    utils::clone::structured_clone,
};

use super::{ReadableStream, ReadableStreamController, ReadableStreamControllerOwnedBorrowMut};

impl<'js> ReadableStream<'js> {
    pub(super) fn readable_stream_tee(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, Self>,
        controller: ReadableStreamControllerOwnedBorrowMut<'js>,
        clone_for_branch_2: bool,
    ) -> Result<(Class<'js, Self>, Class<'js, Self>)> {
        match controller {
            // If stream.[[controller]] implements ReadableByteStreamController, return ? ReadableByteStreamTee(stream).
            ReadableStreamControllerOwnedBorrowMut::ReadableStreamByteController(_controller) => {
                Self::readable_byte_stream_tee(ctx, stream)
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
                        Self::readable_stream_default_cancel_1_algorithm(
                            reason.ctx().clone(),
                            stream,
                            controller,
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
                        let reader = OwnedBorrowMut::from_class(reader);
                        Self::readable_stream_default_cancel_2_algorithm(
                            reason.ctx().clone(),
                            stream,
                            controller,
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

        // Set branch1 to ! CreateReadableStream(startAlgorithm, pullAlgorithm, cancel1Algorithm).
        let (branch_1, controller_1) = {
            let s = Self::create_readable_stream(
                ctx.clone(),
                start_algorithm.clone(),
                pull_algorithm.clone(),
                cancel_algorithm_1,
                None,
                None,
            )?;
            _ = branch_1.set(s.clone());
            let c = s.borrow().controller.clone().unwrap();
            (s, c)
        };

        // Set branch2 to ! CreateReadableStream(startAlgorithm, pullAlgorithm, cancel2Algorithm).
        let (branch_2, controller_2) = {
            let s = Self::create_readable_stream(
                ctx.clone(),
                start_algorithm,
                pull_algorithm,
                cancel_algorithm_2,
                None,
                None,
            )?;
            _ = branch_2.set(s.clone());
            let c = s.borrow().controller.clone().unwrap();
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
                        match ReadableStreamControllerOwnedBorrowMut::from_class(controller_1) {
                            ReadableStreamControllerOwnedBorrowMut::ReadableStreamDefaultController(c) => {
                                let mut stream_1 = OwnedBorrowMut::from_class(branch_1);
                                let reader_1 = stream_1.reader_mut();
                                ReadableStreamDefaultController::readable_stream_default_controller_error(
                                    &ctx,
                                    stream_1,
                                    c,
                                    reader_1,
                                    reason.clone(),
                                )?;
                            },
                            _ => {
                                panic!("ReadableStream created with BYOB controller in default tee")
                            },
                        }

                        // Perform ! ReadableStreamDefaultControllerError(branch2.[[controller]], r).
                        match ReadableStreamControllerOwnedBorrowMut::from_class(controller_2) {
                            ReadableStreamControllerOwnedBorrowMut::ReadableStreamDefaultController(c) => {
                                let mut stream_2 = OwnedBorrowMut::from_class(branch_2);
                                let reader_2 = stream_2.reader_mut();
                                ReadableStreamDefaultController::readable_stream_default_controller_error(
                                    &ctx, stream_2, c, reader_2, reason,
                                )?;
                            },
                            _ => {
                                panic!("ReadableStream created with BYOB controller in default tee")
                            },
                        }
                        // If canceled1 is false or canceled2 is false, resolve cancelPromise with undefined.
                        if reason_1.get().is_none() || reason_2.get().is_none() {
                            resolve_cancel_promise.call((Value::new_undefined(ctx),))?;
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
                            let chunk_2 = if !reason_2.get().is_some() && clone_for_branch_2 {
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
                                        resolve_cancel_promise.call((promise,))?;
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
                            if !reason_1.get().is_some() {
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
                            if !reason_2.get().is_some() {
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
                    if !reason_1.get().is_some() {
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
                    if !reason_2.get().is_some() {
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
                    if !reason_1.get().is_some() || !reason_2.get().is_some() {
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
                    reason_1.get().map(|r| r.trace(tracer));
                    reason_2.get().map(|r| r.trace(tracer));
                    branch_1.get().map(|b| b.trace(tracer));
                    branch_2.get().map(|b| b.trace(tracer));
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
    fn readable_stream_default_cancel_1_algorithm(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: OwnedBorrowMut<'js, ReadableStreamDefaultController<'js>>,
        reader: OwnedBorrowMut<'js, ReadableStreamDefaultReader<'js>>,
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
                controller.into(),
                Some(reader.into()),
                composite_reason.into_js(&ctx)?,
            )?;
            // Resolve cancelPromise with cancelResult.
            resolve_cancel_promise.call((cancel_result,))?;
        }

        // Return cancelPromise.
        Ok(cancel_promise)
    }

    // Let cancel2Algorithm be the following steps, taking a reason argument:
    fn readable_stream_default_cancel_2_algorithm(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, ReadableStream<'js>>,
        controller: OwnedBorrowMut<'js, ReadableStreamDefaultController<'js>>,
        reader: OwnedBorrowMut<'js, ReadableStreamDefaultReader<'js>>,
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
                controller.into(),
                Some(reader.into()),
                composite_reason.into_js(&ctx)?,
            )?;
            // Resolve cancelPromise with cancelResult.
            resolve_cancel_promise.call((cancel_result,))?;
        }

        // Return cancelPromise.
        Ok(cancel_promise)
    }

    fn readable_byte_stream_tee(
        _ctx: Ctx<'js>,
        _stream: OwnedBorrowMut<'js, Self>,
    ) -> Result<(Class<'js, Self>, Class<'js, Self>)> {
        unimplemented!()
    }
}
