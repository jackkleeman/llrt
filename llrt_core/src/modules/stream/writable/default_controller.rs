use rquickjs::{
    class::{OwnedBorrowMut, Trace},
    Class, Ctx, Error, Object, Promise, Result, Value,
};

use crate::modules::{
    events::abort_controller::AbortController,
    stream::{Null, SizeAlgorithm, Undefined},
};

use super::{
    default_writer::WritableStreamDefaultWriter, UnderlyingSink, WritableStream,
    WritableStreamState,
};

#[rquickjs::class]
#[derive(Trace)]
pub(super) struct WritableStreamDefaultController<'js> {
    queue_total_size: f64,
    pub(super) started: bool,
    strategy_hwm: f64,
    strategy_size_algorithm: SizeAlgorithm<'js>,
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

    pub(super) fn writable_stream_default_controller_get_desired_size(&self) -> f64 {
        self.strategy_hwm - self.queue_total_size
    }

    pub(super) fn writable_stream_default_controller_get_chunk_size(
        ctx: Ctx<'js>,
        mut stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, Self>,
        mut writer: OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>,
        chunk: Value<'js>,
    ) -> Result<(
        Value<'js>,
        OwnedBorrowMut<'js, WritableStream<'js>>,
        OwnedBorrowMut<'js, Self>,
        OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>,
    )> {
        let (return_value, stream_class, controller_class, writer_class) =
            Self::strategy_size_algorithm(ctx.clone(), stream, controller, writer, chunk);

        // Let returnValue be the result of performing controller.[[strategySizeAlgorithm]], passing in chunk, and interpreting the result as a completion record.
        match return_value {
            Ok(chunk_size) => {
                stream = OwnedBorrowMut::from_class(stream_class);
                controller = OwnedBorrowMut::from_class(controller_class);
                writer = OwnedBorrowMut::from_class(writer_class);
                Ok((chunk_size, stream, controller, writer))
            },
            // If returnValue is an abrupt completion,
            Err(Error::Exception) => {
                let reason = ctx.catch();

                stream = OwnedBorrowMut::from_class(stream_class);
                controller = OwnedBorrowMut::from_class(controller_class);
                writer = OwnedBorrowMut::from_class(writer_class);

                // Perform ! WritableStreamDefaultControllerErrorIfNeeded(controller, returnValue.[[Value]]).
                (stream, controller, writer) =
                    Self::writable_stream_default_controller_error_if_needed(
                        ctx.clone(),
                        stream,
                        controller,
                        writer,
                        reason,
                    )?;

                // Return 1.
                Ok((Value::new_number(ctx, 1.0), stream, controller, writer))
            },
            Err(err) => Err(err),
        }
    }

    pub(super) fn writable_stream_default_controller_error_if_needed(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        controller: OwnedBorrowMut<'js, Self>,
        writer: OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>,
        error: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, WritableStream<'js>>,
        OwnedBorrowMut<'js, Self>,
        OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>,
    )> {
        // If controller.[[stream]].[[state]] is "writable", perform ! WritableStreamDefaultControllerError(controller, error).
        if let WritableStreamState::Writable = stream.state {
            Self::writable_stream_default_controller_error(ctx, stream, controller, writer, error)
        } else {
            Ok((stream, controller, writer))
        }
    }

    pub(super) fn writable_stream_default_controller_error(
        ctx: Ctx<'js>,
        // Let stream be controller.[[stream]].
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        mut controller: OwnedBorrowMut<'js, Self>,
        writer: OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>,
        reason: Value<'js>,
    ) -> Result<(
        OwnedBorrowMut<'js, WritableStream<'js>>,
        OwnedBorrowMut<'js, Self>,
        OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>,
    )> {
        // Perform ! WritableStreamDefaultControllerClearAlgorithms(controller).
        controller.writable_stream_default_controller_clear_algorithms();

        // Perform ! WritableStreamStartErroring(stream, error).
        let (stream, controller, writer) = WritableStream::writable_stream_start_erroring(
            ctx,
            stream,
            controller,
            Some(writer),
            reason,
        )?;

        Ok((
            stream,
            controller,
            writer.expect("writable_stream_start_erroring must return the provided writer"),
        ))
    }

    pub(super) fn writable_stream_default_controller_clear_algorithms(&mut self) {
        unimplemented!()
    }

    pub(super) fn writable_stream_default_controller_write(
        &self,
        chunk: Value<'js>,
        chunk_size: Value<'js>,
    ) -> Result<()> {
        unimplemented!()
    }

    pub(super) fn error_steps(&mut self) -> Result<()> {
        unimplemented!()
    }

    pub(super) fn abort_steps(&mut self, reason: Value<'js>) -> Result<Promise<'js>> {
        unimplemented!()
    }

    fn strategy_size_algorithm(
        ctx: Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
        controller: OwnedBorrowMut<'js, Self>,
        writer: OwnedBorrowMut<'js, WritableStreamDefaultWriter<'js>>,
        chunk: Value<'js>,
    ) -> (
        Result<Value<'js>>,
        Class<'js, WritableStream<'js>>,
        Class<'js, Self>,
        Class<'js, WritableStreamDefaultWriter<'js>>,
    ) {
        let strategy_size_algorithm = controller.strategy_size_algorithm.clone();

        let stream_class = stream.into_inner();
        let controller_class = controller.into_inner();
        let writer_class = writer.into_inner();

        (
            strategy_size_algorithm.call(ctx, chunk),
            stream_class,
            controller_class,
            writer_class,
        )
    }
}
