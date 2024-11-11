use std::{cell::Cell, rc::Rc};

use llrt_modules::ModuleInfo;
use llrt_utils::module::export_default;
use readable::{
    ByteLengthQueuingStrategy, CountQueuingStrategy, ReadableByteStreamController, ReadableStream,
    ReadableStreamBYOBRequest, ReadableStreamDefaultReader,
};
use rquickjs::{
    class::{JsClass, OwnedBorrow, OwnedBorrowMut, Trace, Tracer},
    module::{Declarations, Exports, ModuleDef},
    prelude::{OnceFn, This},
    Class, Ctx, Error, Exception, FromJs, Function, IntoAtom, IntoJs, Object, Promise, Result,
    Type, Value,
};
use writable::WritableStream;

mod readable;
mod writable;

struct ReadableWritablePair<'js> {
    readable: Class<'js, ReadableStream<'js>>,
    writeable: Class<'js, WritableStream<'js>>,
}

impl<'js> FromJs<'js> for ReadableWritablePair<'js> {
    fn from_js(_ctx: &rquickjs::Ctx<'js>, value: rquickjs::Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let readable = obj.get::<_, Class<'js, ReadableStream>>("readable")?;
        let writeable = obj.get::<_, Class<'js, WritableStream>>("writeable")?;

        Ok(Self {
            readable,
            writeable,
        })
    }
}

pub struct StreamModule;

impl ModuleDef for StreamModule {
    fn declare(declare: &Declarations) -> Result<()> {
        declare.declare(stringify!(ReadableStream))?;
        declare.declare(stringify!(ByteLengthQueuingStrategy))?;
        declare.declare(stringify!(CountQueuingStrategy))?;
        declare.declare(stringify!(ReadableStreamDefaultReader))?;
        declare.declare(stringify!(ReadableByteStreamController))?;
        declare.declare(stringify!(ReadableStreamBYOBRequest))?;

        declare.declare("default")?;
        Ok(())
    }

    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
        export_default(ctx, exports, |default| {
            Class::<ReadableStream>::define(default)?;
            Class::<ByteLengthQueuingStrategy>::define(default)?;
            Class::<CountQueuingStrategy>::define(default)?;
            Class::<ReadableStreamDefaultReader>::define(default)?;
            Class::<ReadableByteStreamController>::define(default)?;
            Class::<ReadableStreamBYOBRequest>::define(default)?;

            Ok(())
        })?;

        Ok(())
    }
}

impl From<StreamModule> for ModuleInfo<StreamModule> {
    fn from(val: StreamModule) -> Self {
        ModuleInfo {
            name: "stream/web",
            module: val,
        }
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

impl<'js, T: IntoJs<'js>> IntoJs<'js> for Undefined<T> {
    fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
        match self.0 {
            None => Ok(Value::new_undefined(ctx.clone())),
            Some(val) => val.into_js(ctx),
        }
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

fn promise_rejected_with<'js>(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Promise<'js>> {
    let (promise, _, reject) = Promise::new(ctx)?;
    let () = reject.call((value,))?;

    Ok(promise)
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

fn set_promise_is_handled_to_true<'js>(ctx: Ctx<'js>, promise: &Promise<'js>) -> Result<()> {
    promise.then()?.call((
        This(promise.clone()),
        Value::new_undefined(ctx.clone()),
        Function::new(ctx, || {}),
    ))
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
