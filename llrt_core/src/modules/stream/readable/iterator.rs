use std::{
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

use rquickjs::{
    atom::PredefinedAtom,
    class::{ClassId, JsClass, OwnedBorrow, OwnedBorrowMut, Trace},
    function::Constructor,
    methods,
    prelude::{OnceFn, Opt, This},
    Atom, Class, Ctx, Error, FromJs, Function, IntoAtom, IntoJs, Object, Promise, Result, Symbol,
    Type, Value,
};

use super::{
    class_from_owned_borrow_mut, promise_resolved_with, upon_promise, ReadableStream,
    ReadableStreamController, ReadableStreamControllerOwnedBorrowMut, ReadableStreamDefaultReader,
    ReadableStreamGenericReader, ReadableStreamReadRequest, ReadableStreamReadResult,
    ReadableStreamReaderOwnedBorrowMut,
};

pub(super) struct SymbolAsyncIterator;

impl<'js> IntoAtom<'js> for SymbolAsyncIterator {
    fn into_atom(self, ctx: &Ctx<'js>) -> Result<Atom<'js>> {
        // hack until PredefinedAtom::SymbolAsyncIterator is available
        Ok(Atom::from_predefined(ctx.clone(), unsafe {
            // SAFETY: PredefinedAtom is repr(u32)
            std::mem::transmute(rquickjs::qjs::JS_ATOM_Symbol_asyncIterator as u32)
        }))
    }
}

pub(super) enum IteratorKind {
    // Sync,
    Async,
}

pub(super) struct IteratorRecord<'js> {
    pub(super) iterator: Object<'js>,
    next_method: Function<'js>,
    done: bool,
}

impl<'js> IteratorRecord<'js> {
    pub(super) fn get_iterator(
        ctx: &Ctx<'js>,
        obj: Value<'js>,
        kind: IteratorKind,
    ) -> Result<Self> {
        let method: Option<Function<'js>> = match kind {
            // If kind is async, then
            IteratorKind::Async => {
                // Let method be ? GetMethod(obj, %Symbol.asyncIterator%).
                let method: Option<Function<'js>> = ctx
                    .eval::<Function<'js>, _>(
                        "(obj) => obj == null ? undefined : obj[Symbol.asyncIterator]",
                    )?
                    .call((obj.clone(),))?;
                // If method is undefined, then
                if method.is_none() {
                    // Let syncMethod be ? GetMethod(obj, %Symbol.iterator%).
                    let sync_method: Option<Function<'js>> = ctx
                        .eval::<Function<'js>, _>(
                            "(obj) => obj == null ? undefined : obj[Symbol.iterator]",
                        )?
                        .call((obj.clone(),))?;
                    // If syncMethod is undefined, throw a TypeError exception.
                    let sync_method = match sync_method {
                        None => {
                            let e: Value =
                                ctx.eval(r#"new TypeError("Object is not an iterator")"#)?;
                            return Err(ctx.throw(e));
                        },
                        Some(sync_method) => sync_method,
                    };
                    // Let syncIteratorRecord be ? GetIteratorFromMethod(obj, syncMethod).
                    let sync_iterator_record =
                        Self::get_iterator_from_method(ctx, &obj, sync_method)?;

                    // Return CreateAsyncFromSyncIterator(syncIteratorRecord).
                    return sync_iterator_record.create_async_from_sync_iterator(ctx);
                }

                method
            },
            // IteratorKind::Sync => ctx
            //     .eval::<Function<'js>, _>(
            //         "(obj) => obj == null ? undefined : obj[Symbol.iterator]",
            //     )?
            //     .call((obj.clone(),))?,
        };

        // If method is undefined, throw a TypeError exception.
        match method {
            None => {
                let e: Value = ctx.eval(r#"new TypeError("Object is not an iterator")"#)?;
                return Err(ctx.throw(e));
            },
            Some(method) => {
                // Return ? GetIteratorFromMethod(obj, method).
                Self::get_iterator_from_method(ctx, &obj, method)
            },
        }
    }

    fn get_iterator_from_method(
        ctx: &Ctx<'js>,
        obj: &Value<'js>,
        method: Function<'js>,
    ) -> Result<Self> {
        // Let iterator be ? Call(method, obj).
        let iterator: Value<'js> = method.call((This(obj),))?;
        let iterator = match iterator.into_object() {
            Some(iterator) => iterator,
            None => {
                let e: Value =
                    ctx.eval(r#"new TypeError("The iterator method must return an object")"#)?;
                return Err(ctx.throw(e));
            },
        };
        // Let nextMethod be ? Get(iterator, "next").
        let next_method = iterator.get(PredefinedAtom::Next)?;
        // Let iteratorRecord be the Iterator Record { [[Iterator]]: iterator, [[NextMethod]]: nextMethod, [[Done]]: false }.
        // Return iteratorRecord.
        Ok(Self {
            iterator,
            next_method,
            done: false,
        })
    }

    fn create_async_from_sync_iterator(self, ctx: &Ctx<'js>) -> Result<Self> {
        let sync_iterable = Object::new(ctx.clone())?;
        sync_iterable.set(
            Symbol::iterator(ctx.clone()),
            Function::new(ctx.clone(), {
                let iterator = self.iterator.clone();
                move || iterator.clone()
            }),
        )?;
        let async_iterator: Object<'js> = ctx
            .eval::<Function<'js>, _>(
                r#"
            (syncIterable) => (async function* () {
              return yield* syncIterable;
            })()
        "#,
            )?
            .call((sync_iterable,))?;

        let next_method = async_iterator.get(PredefinedAtom::Next)?;

        Ok(Self {
            iterator: async_iterator,
            next_method,
            done: false,
        })
    }

    pub(super) fn iterator_next(
        &mut self,
        ctx: &Ctx<'js>,
        value: Option<Value<'js>>,
    ) -> Result<Object<'js>> {
        let result: Result<Value<'js>> = match value {
            // If value is not present, then
            None => {
                // Let result be Completion(Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]])).
                let r = self.next_method.call((This(self.iterator.clone()),));
                r
            },
            // Else,
            Some(value) => {
                // Let result be Completion(Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]], « value »)).
                self.next_method.call((This(self.iterator.clone()), value))
            },
        };

        let result = match result {
            // If result is a throw completion, then
            Err(Error::Exception) => {
                // Set iteratorRecord.[[Done]] to true.
                self.done = true;
                // Return ? result.
                return Err(Error::Exception);
            },
            Err(err) => return Err(err),
            // Set result to ! result.
            Ok(result) => result,
        };

        let result = match result.into_object() {
            // If result is not an Object, then
            None => {
                // Set iteratorRecord.[[Done]] to true.
                self.done = true;
                let e: Value = ctx
                    .eval(r#"new TypeError("The iterator.next() method must return an object")"#)?;
                return Err(ctx.throw(e));
            },
            Some(result) => result,
        };
        // Return result.
        Ok(result)
    }

    pub(super) fn iterator_complete(iterator_result: &Object<'js>) -> Result<bool> {
        let done: Value<'js> = iterator_result.get(PredefinedAtom::Done)?;
        Ok(match done.type_of() {
            Type::Bool => done.as_bool().unwrap(),
            Type::Undefined => false,
            Type::Null => false,
            Type::Float => match done.as_float().unwrap() {
                0.0 => false,
                val if val.is_nan() => false,
                _ => true,
            },
            Type::Int => match done.as_int().unwrap() {
                0 => false,
                _ => true,
            },
            Type::String => match done.as_string().unwrap().to_string().as_deref() {
                Ok("") => false,
                _ => true,
            },
            _ => true,
        })
    }

    pub(super) fn iterator_value(iterator_result: &Object<'js>) -> Result<Value<'js>> {
        iterator_result.get(PredefinedAtom::Value)
    }
}

#[derive(Trace)]
pub(super) struct ReadableStreamAsyncIterator<'js> {
    stream: Class<'js, ReadableStream<'js>>,
    controller: ReadableStreamController<'js>,
    reader: Class<'js, ReadableStreamDefaultReader<'js>>,
    prevent_cancel: bool,
    eoi_symbol: Symbol<'js>,
    #[qjs(skip_trace)]
    is_finished: Rc<AtomicBool>,
    #[qjs(skip_trace)]
    ongoing_promise: Option<Promise<'js>>,
}

impl<'js> ReadableStreamAsyncIterator<'js> {
    pub(super) fn new(
        ctx: Ctx<'js>,
        stream: Class<'js, ReadableStream<'js>>,
        controller: ReadableStreamController<'js>,
        reader: Class<'js, ReadableStreamDefaultReader<'js>>,
        prevent_cancel: bool,
    ) -> Result<Class<'js, Self>> {
        let eoi_symbol = ctx.eval("Symbol('async iterator end of iteration')")?;
        Class::instance(
            ctx,
            Self {
                stream,
                controller,
                reader,
                prevent_cancel,
                eoi_symbol,
                is_finished: Rc::new(AtomicBool::new(false)),
                ongoing_promise: None,
            },
        )
    }
}

impl<'js> JsClass<'js> for ReadableStreamAsyncIterator<'js> {
    const NAME: &'static str = "ReadableStreamAsyncIterator";
    type Mutable = rquickjs::class::Writable;
    fn class_id() -> &'static ClassId {
        static ID: ClassId = ClassId::new();
        &ID
    }
    fn prototype(ctx: &Ctx<'js>) -> Result<Option<Object<'js>>> {
        use rquickjs::class::impl_::MethodImplementor;
        let proto = rquickjs::Object::new(ctx.clone())?;
        let async_iterator_prototype: Object<'js> = ctx.eval(
            r#"
            Object.getPrototypeOf(
              Object.getPrototypeOf(Object.getPrototypeOf((async function* () {})())),
            )
            "#,
        )?;
        proto.set_prototype(Some(&async_iterator_prototype))?;
        let implementor = rquickjs::class::impl_::MethodImpl::<Self>::new();
        (&implementor).implement(&proto)?;
        let next_fn: Function<'js> = proto.get("next")?;
        // yup, the wpt tests really do check these.
        next_fn.set_name("next")?;
        let return_fn: Function<'js> = proto.get("return")?;
        return_fn.set_name("return")?;
        return_fn.set_length(1)?;
        Ok(Some(proto))
    }
    fn constructor(ctx: &Ctx<'js>) -> Result<Option<Constructor<'js>>> {
        use rquickjs::class::impl_::ConstructorCreator;
        let implementor = rquickjs::class::impl_::ConstructorCreate::<Self>::new();
        (&implementor).create_constructor(ctx)
    }
}
impl<'js> IntoJs<'js> for ReadableStreamAsyncIterator<'js> {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<rquickjs::Value<'js>> {
        let cls = Class::<Self>::instance(ctx.clone(), self)?;
        rquickjs::IntoJs::into_js(cls, ctx)
    }
}

impl<'js> FromJs<'js> for ReadableStreamAsyncIterator<'js>
where
    for<'a> rquickjs::class::impl_::CloneWrapper<'a, Self>:
        rquickjs::class::impl_::CloneTrait<Self>,
{
    fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        use rquickjs::class::impl_::CloneTrait;
        let value = Class::<Self>::from_js(ctx, value)?;
        let borrow = value.try_borrow()?;
        Ok(rquickjs::class::impl_::CloneWrapper(&*borrow).wrap_clone())
    }
}

#[methods]
impl<'js> ReadableStreamAsyncIterator<'js> {
    fn next(ctx: Ctx<'js>, iterator: This<OwnedBorrowMut<'js, Self>>) -> Result<Promise<'js>> {
        let is_finished = iterator.is_finished.clone();

        let next_steps = move |ctx: Ctx<'js>, iterator: &Self, iterator_class: Class<'js, Self>| {
            if is_finished.load(Ordering::Relaxed) {
                return promise_resolved_with(
                    &ctx,
                    Ok(ReadableStreamReadResult {
                        value: None,
                        done: true,
                    }
                    .into_js(&ctx)?),
                );
            }

            let next_promise = Self::next_steps(&ctx, iterator)?;

            upon_promise(
                ctx,
                next_promise,
                move |ctx, result: std::result::Result<Value<'js>, _>| {
                    let mut iterator = OwnedBorrowMut::from_class(iterator_class);
                    match result {
                        Ok(next) => {
                            iterator.ongoing_promise = None;
                            if next.as_symbol() == Some(&iterator.eoi_symbol) {
                                iterator.is_finished.store(true, Ordering::Relaxed);
                                Ok(ReadableStreamReadResult {
                                    value: None,
                                    done: true,
                                })
                            } else {
                                Ok(ReadableStreamReadResult {
                                    value: Some(next),
                                    done: false,
                                })
                            }
                        },
                        Err(reason) => {
                            iterator.ongoing_promise = None;
                            iterator.is_finished.store(true, Ordering::Relaxed);
                            Err(ctx.throw(reason))
                        },
                    }
                },
            )
        };

        let (iterator_class, mut iterator) = class_from_owned_borrow_mut(iterator.0);
        let ongoing_promise = iterator.ongoing_promise.take();

        let ongoing_promise = match ongoing_promise {
            Some(ongoing_promise) => upon_promise(
                ctx,
                ongoing_promise,
                move |ctx, _: std::result::Result<Value<'js>, _>| {
                    let iterator = OwnedBorrow::from_class(iterator_class.clone());
                    next_steps(ctx, &iterator, iterator_class)
                },
            )?,
            None => next_steps(ctx, &iterator, iterator_class)?,
        };

        Ok(iterator.ongoing_promise.insert(ongoing_promise).clone())
    }

    #[qjs(rename = "return")]
    fn r#return(
        ctx: Ctx<'js>,
        iterator: This<OwnedBorrowMut<'js, Self>>,
        value: Opt<Value<'js>>,
    ) -> Result<Promise<'js>> {
        let is_finished = iterator.is_finished.clone();
        let value = value.0.unwrap_or(Value::new_undefined(ctx.clone()));

        let return_steps = {
            let value = value.clone();
            move |ctx: Ctx<'js>, iterator: &Self| {
                if is_finished.swap(true, Ordering::Relaxed) {
                    return promise_resolved_with(
                        &ctx,
                        Ok(ReadableStreamReadResult {
                            value: Some(value),
                            done: true,
                        }
                        .into_js(&ctx)?),
                    );
                }

                return Self::return_steps(ctx.clone(), &iterator, value);
            }
        };

        let (iterator_class, mut iterator) = class_from_owned_borrow_mut(iterator.0);
        let ongoing_promise = iterator.ongoing_promise.take();

        let ongoing_promise = match ongoing_promise {
            Some(ongoing_promise) => upon_promise(
                ctx.clone(),
                ongoing_promise,
                move |ctx, _: std::result::Result<Value<'js>, _>| {
                    let iterator = OwnedBorrow::from_class(iterator_class.clone());
                    return_steps(ctx, &iterator)
                },
            )?,
            None => return_steps(ctx.clone(), &iterator)?,
        };

        iterator.ongoing_promise = Some(ongoing_promise.clone());

        ongoing_promise.then()?.call((
            This(ongoing_promise.clone()),
            Function::new(
                ctx,
                OnceFn::new(move || ReadableStreamReadResult {
                    value: Some(value),
                    done: true,
                }),
            ),
        ))
    }
}

impl<'js> ReadableStreamAsyncIterator<'js> {
    // The get the next iteration result steps for a ReadableStream, given stream and iterator, are:
    fn next_steps(ctx: &Ctx<'js>, iterator: &Self) -> Result<Promise<'js>> {
        let stream = iterator.stream.clone();
        let controller = iterator.controller.clone();
        let eoi_symbol = iterator.eoi_symbol.clone();
        // Let reader be iterator’s reader.
        let reader = iterator.reader.clone();
        // Let promise be a new promise.
        let (promise, resolve, reject) = Promise::new(&ctx)?;
        // Let readRequest be a new read request with the following items:
        let read_request = ReadableStreamReadRequest {
            chunk_steps: Box::new({
                let resolve = resolve.clone();
                move |stream, controller, reader, chunk| {
                    // Resolve promise with chunk.
                    resolve.call((chunk,))?;
                    Ok((stream, controller, reader))
                }
            }),
            close_steps: Box::new({
                let resolve = resolve.clone();
                move |ctx, mut stream, mut controller, reader| {
                    let mut reader = match reader {
                        Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(
                            r,
                        )) => r,
                        Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(_)) => {
                            panic!(
                                "ReadableStreamAsyncIterator close steps called with byob reader"
                            )
                        },
                        None => {
                            panic!("ReadableStreamAsyncIterator close steps called without reader")
                        },
                    };
                    // Perform ! ReadableStreamDefaultReaderRelease(reader).
                    (stream, controller, reader) =
                        ReadableStreamDefaultReader::readable_stream_default_reader_release(
                            ctx, stream, controller, reader,
                        )?;
                    // Resolve promise with end of iteration.
                    resolve.call((eoi_symbol,))?;
                    Ok((stream, controller, Some(reader.into())))
                }
            }),
            error_steps: Box::new(move |mut stream, mut controller, reader, e| {
                let mut reader = match reader {
                    Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamDefaultReader(r)) => r,
                    Some(ReadableStreamReaderOwnedBorrowMut::ReadableStreamBYOBReader(_)) => {
                        panic!("ReadableStreamAsyncIterator error steps called with byob reader")
                    },
                    None => {
                        panic!("ReadableStreamAsyncIterator error steps called without reader")
                    },
                };
                // Perform ! ReadableStreamDefaultReaderRelease(reader).
                (stream, controller, reader) =
                    ReadableStreamDefaultReader::readable_stream_default_reader_release(
                        e.ctx(),
                        stream,
                        controller,
                        reader,
                    )?;
                // Reject promise with e.
                reject.call((e,))?;
                Ok((stream, controller, Some(reader.into())))
            }),
            trace: Box::new({
                let reader = reader.clone();
                move |tracer| reader.trace(tracer)
            }),
        };

        let stream = OwnedBorrowMut::from_class(stream);
        let controller = ReadableStreamControllerOwnedBorrowMut::from_class(controller);
        let reader = OwnedBorrowMut::from_class(reader);

        // Perform ! ReadableStreamDefaultReaderRead(this, readRequest).
        ReadableStreamDefaultReader::readable_stream_default_reader_read(
            &ctx,
            stream,
            controller,
            reader,
            read_request,
        )?;

        // Return promise.
        Ok(promise)
    }

    // The asynchronous iterator return steps for a ReadableStream, given stream, iterator, and arg, are:
    fn return_steps(ctx: Ctx<'js>, iterator: &Self, arg: Value<'js>) -> Result<Promise<'js>> {
        let stream = OwnedBorrowMut::from_class(iterator.stream.clone());
        let controller =
            ReadableStreamControllerOwnedBorrowMut::from_class(iterator.controller.clone());
        // Let reader be iterator’s reader.
        let reader = OwnedBorrowMut::from_class(iterator.reader.clone());

        // If iterator’s prevent cancel is false:
        if !iterator.prevent_cancel {
            // Let result be ! ReadableStreamReaderGenericCancel(reader, arg).
            let (result, stream, controller, reader) =
                ReadableStreamGenericReader::readable_stream_reader_generic_cancel(
                    ctx.clone(),
                    stream,
                    controller,
                    reader.into(),
                    arg,
                )?;

            // Perform ! ReadableStreamDefaultReaderRelease(reader).
            ReadableStreamDefaultReader::readable_stream_default_reader_release(
                &ctx,
                stream,
                controller,
                reader.into_default_reader().expect(
                    "readable_stream_reader_generic_cancel must return the same type of reader",
                ),
            )?;

            // Return result.
            return Ok(result);
        }

        // Perform ! ReadableStreamDefaultReaderRelease(reader).
        ReadableStreamDefaultReader::readable_stream_default_reader_release(
            &ctx, stream, controller, reader,
        )?;

        // Return a promise resolved with undefined.
        promise_resolved_with(&ctx.clone(), Ok(Value::new_undefined(ctx)))
    }
}
