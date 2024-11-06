use llrt_modules::ModuleInfo;
use llrt_utils::module::export_default;
use readable::{
    ByteLengthQueuingStrategy, CountQueuingStrategy, ReadableByteStreamController, ReadableStream,
    ReadableStreamBYOBRequest, ReadableStreamDefaultReader,
};
use rquickjs::{
    module::{Declarations, Exports, ModuleDef},
    Class, Ctx, Error, FromJs, Result,
};
use writeable::WriteableStream;

mod readable;
mod writeable;

struct ReadableWritablePair<'js> {
    readable: Class<'js, ReadableStream<'js>>,
    writeable: Class<'js, WriteableStream>,
}

impl<'js> FromJs<'js> for ReadableWritablePair<'js> {
    fn from_js(_ctx: &rquickjs::Ctx<'js>, value: rquickjs::Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let readable = obj.get::<_, Class<'js, ReadableStream>>("readable")?;
        let writeable = obj.get::<_, Class<'js, WriteableStream>>("writeable")?;

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
