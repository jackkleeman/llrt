use rquickjs::{class::Trace, methods, Ctx, Error, FromJs, Function, Result, Value};

use super::ObjectExt;

#[derive(Trace)]
#[rquickjs::class]
pub(crate) struct CountQueuingStrategy<'js> {
    high_water_mark: f64,
    size: Function<'js>,
}

#[methods(rename_all = "camelCase")]
impl<'js> CountQueuingStrategy<'js> {
    #[qjs(constructor)]
    fn new(ctx: Ctx<'js>, init: QueueingStrategyInit) -> Result<Self> {
        // Set this.[[highWaterMark]] to init["highWaterMark"].
        Ok(Self {
            high_water_mark: init.high_water_mark,
            size: Function::new(ctx, count_queueing_strategy_size_function)?,
        })
    }

    #[qjs(get)]
    fn size(&self) -> Function<'js> {
        self.size.clone()
    }

    #[qjs(get)]
    fn high_water_mark(&self) -> f64 {
        self.high_water_mark
    }
}

fn count_queueing_strategy_size_function() -> f64 {
    // Return 1.
    1.0
}

struct QueueingStrategyInit {
    high_water_mark: f64,
}

impl<'js> FromJs<'js> for QueueingStrategyInit {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let high_water_mark = obj
            .get_optional("highWaterMark")?
            .ok_or(Error::new_from_js(ty_name, "QueueingStrategyInit"))?;

        Ok(Self { high_water_mark })
    }
}
