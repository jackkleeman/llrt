use rquickjs::{
    class::{OwnedBorrowMut, Trace},
    Class, Ctx, Object, Promise, Result, Value,
};

use crate::modules::{
    events::abort_controller::AbortController,
    stream::{Null, SizeAlgorithm, Undefined},
};

use super::{default_writer::WritableStreamDefaultWriter, UnderlyingSink, WritableStream};

#[rquickjs::class]
#[derive(Trace)]
pub(super) struct WritableStreamDefaultController<'js> {
    pub(super) started: bool,
    pub(super) abort_controller: Class<'js, AbortController<'js>>,
}

impl<'js> WritableStreamDefaultController<'js> {
    pub(super) fn set_up_writable_stream_default_controller_from_underlying_sink(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        underlying_sink: Null<Undefined<Object<'js>>>,
        underlying_sink_dict: UnderlyingSink<'js>,
        high_water_mark: f64,
        size_algorithm: SizeAlgorithm<'js>,
    ) -> Result<()> {
        unimplemented!()
    }

    pub(super) fn writable_stream_default_controller_close(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        controller: OwnedBorrowMut<'js, WritableStreamDefaultController<'js>>,
        writer: Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
    ) -> Result<(
        OwnedBorrowMut<'js, WritableStream<'js>>,
        OwnedBorrowMut<'js, Self>,
        Option<OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>>,
    )> {
        unimplemented!()
    }

    pub(super) fn error_steps(&mut self) -> Result<()> {
        unimplemented!()
    }

    pub(super) fn abort_steps(&mut self, reason: Value<'js>) -> Result<Promise<'js>> {
        unimplemented!()
    }
}
