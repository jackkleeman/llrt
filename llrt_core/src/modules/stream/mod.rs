use readable::ReadableStream;
use rquickjs::{Class, Error, FromJs, Result};
use writeable::WriteableStream;

mod readable;
mod writeable;

struct ReadableWritablePair<'js> {
    readable: Class<'js, readable::ReadableStream<'js>>,
    writeable: Class<'js, writeable::WriteableStream>,
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
