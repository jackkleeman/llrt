use rquickjs::{class::Trace, Class, Result, Value};

use super::{ReadableStream, SizeAlgorithm, UnderlyingSource};

#[derive(Trace)]
#[rquickjs::class]
pub(super) struct ReadableStreamDefaultController {}

impl ReadableStreamDefaultController {
    pub(super) fn set_up_readable_stream_default_controller_from_underlying_source<'js>(
        stream: Class<'js, ReadableStream<'js>>,
        underlying_source: Option<UnderlyingSource<'js>>,
        high_water_mark: f64,
        size_algorithm: SizeAlgorithm<'js>,
    ) -> Result<()> {
        unimplemented!()
    }

    // readonly attribute unrestricted double? desiredSize;
    fn desired_size(&self) -> Option<f64> {
        unimplemented!()
    }

    // undefined close();
    fn close() {
        unimplemented!()
    }

    // undefined enqueue(optional any chunk);
    fn enqueue(chunk: Value) {
        unimplemented!()
    }

    // undefined error(optional any e);
    fn error(e: Value) {
        unimplemented!()
    }
}
