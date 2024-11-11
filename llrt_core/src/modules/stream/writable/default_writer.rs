use rquickjs::{
    class::{OwnedBorrowMut, Trace},
    Class, Ctx, Function, Result, Value,
};

use crate::modules::stream::set_promise_is_handled_to_true;

use super::{
    ResolveablePromise, WritableStream, __impl_class_WritableStream_::promise_rejected_with,
};

#[rquickjs::class]
#[derive(Trace)]
pub(super) struct WritableStreamDefaultWriter<'js> {
    pub(super) ready_promise: ResolveablePromise<'js>,
    pub(super) closed_promise: ResolveablePromise<'js>,
}

impl<'js> WritableStreamDefaultWriter<'js> {
    pub(super) fn acquire_writable_stream_default_writer(
        ctx: &Ctx<'js>,
        stream: OwnedBorrowMut<'js, WritableStream<'js>>,
    ) -> Result<(OwnedBorrowMut<'js, WritableStream<'js>>, Class<'js, Self>)> {
        unimplemented!()
    }

    pub(super) fn writable_stream_default_writer_ensure_ready_promise_rejected(
        &mut self,
        ctx: &Ctx<'js>,
        error: Value<'js>,
    ) -> Result<()> {
        if todo!() {
            // If writer.[[readyPromise]].[[PromiseState]] is "pending", reject writer.[[readyPromise]] with error.
            let () = self.ready_promise.reject.call((error,))?;
        } else {
            // Otherwise, set writer.[[readyPromise]] to a promise rejected with error.
            let noop_function = Function::new(ctx.clone(), || {})?;
            self.ready_promise = ResolveablePromise {
                promise: promise_rejected_with(ctx, error)?,
                resolve: noop_function.clone(),
                reject: noop_function.clone(),
            };
        }

        // Set writer.[[readyPromise]].[[PromiseIsHandled]] to true.
        set_promise_is_handled_to_true(ctx.clone(), &self.ready_promise.promise)?;
        Ok(())
    }
}
