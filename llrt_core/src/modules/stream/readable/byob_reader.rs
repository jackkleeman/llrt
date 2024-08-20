use llrt_utils::{bytes::ObjectBytes, object::ObjectExt};
use rquickjs::{
    class::Trace, methods, Class, Ctx, Error, FromJs, Function, Promise, Result, Value,
};
use std::collections::VecDeque;

use super::ReadableStreamGenericReader;

#[derive(Trace)]
#[rquickjs::class]
pub(super) struct ReadableStreamBYOBReader<'js> {
    pub(super) generic: ReadableStreamGenericReader<'js>,
    pub(super) read_into_requests: VecDeque<ReadableStreamReadIntoRequest<'js>>,
}

impl<'js> ReadableStreamBYOBReader<'js> {
    pub(super) fn readable_stream_byob_reader_error_read_into_requests(
        &mut self,
        e: Value<'js>,
    ) -> Result<()> {
        // Let readIntoRequests be reader.[[readIntoRequests]].
        let read_into_requests = &mut self.read_into_requests;

        // Set reader.[[readIntoRequests]] to a new empty list.
        let read_into_requests = read_into_requests.split_off(0);
        // For each readIntoRequest of readIntoRequests,
        for read_into_request in read_into_requests {
            // Perform readIntoRequest’s error steps, given e.
            read_into_request.error_steps.call((e.clone(),))?;
        }

        Ok(())
    }
}

#[methods]
impl<'js> ReadableStreamBYOBReader<'js> {
    #[qjs(constructor)]
    fn new() -> Result<Class<'js, Self>> {
        unimplemented!()
    }

    fn read(
        &self,
        view: ObjectBytes<'js>,
        options: Option<ReadableStreamBYOBReaderReadOptions>,
    ) -> Promise<'js> {
        unimplemented!()
    }
}

struct ReadableStreamBYOBReaderReadOptions {
    min: u64,
}

impl<'js> FromJs<'js> for ReadableStreamBYOBReaderReadOptions {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let ty_name = value.type_name();
        let obj = value
            .as_object()
            .ok_or(Error::new_from_js(ty_name, "Object"))?;

        let min = obj.get_optional::<_, _>("min")?.unwrap_or(1);

        Ok(Self { min })
    }
}

#[derive(Trace)]
pub(super) struct ReadableStreamReadIntoRequest<'js> {
    pub(super) chunk_steps: Function<'js>,
    pub(super) close_steps: Function<'js>,
    error_steps: Function<'js>,
}
