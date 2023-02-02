use std::io::Write;

use arrow::array::*;
use arrow::chunk::Chunk;

use crate::ColumnMeta;
use crate::Compression;
use crate::PageMeta;
use arrow::error::Result;

use super::{write, NativeWriter};

use arrow::io::parquet::write::{
    get_max_length, slice_parquet_array, to_leaves, to_nested, to_parquet_leaves, SchemaDescriptor,
};

/// Options declaring the behaviour of writing to IPC
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WriteOptions {
    /// Whether the buffers should be compressed and which codec to use.
    /// Note: to use compression the crate must be compiled with feature `io_ipc_compression`.
    pub compression: Compression,
    pub max_page_size: Option<usize>,
}

impl<W: Write> NativeWriter<W> {
    pub fn encode_chunk(
        &mut self,
        schema_descriptor: SchemaDescriptor,
        chunk: &Chunk<Box<dyn Array>>,
    ) -> Result<()> {
        let page_size = self
            .options
            .max_page_size
            .unwrap_or(chunk.len())
            .min(chunk.len());

        for (array, type_) in chunk
            .arrays()
            .iter()
            .zip(schema_descriptor.fields().to_vec())
        {
            let array = array.as_ref();
            let nested = to_nested(array, &type_)?;
            let types = to_parquet_leaves(type_);
            let leaf_arrays = to_leaves(array);

            for ((leaf_array, nested), type_) in leaf_arrays
                .iter()
                .zip(nested.into_iter())
                .zip(types.into_iter())
            {
                let start = self.writer.offset;
                let length = get_max_length(*leaf_array, &nested);

                let page_metas: Vec<PageMeta> = (0..length)
                    .step_by(page_size)
                    .map(|offset| {
                        let length = if offset + page_size > length {
                            length - offset
                        } else {
                            page_size
                        };

                        let (sub_array, sub_nested) =
                            slice_parquet_array(*leaf_array, &nested, offset, length);

                        let page_start = self.writer.offset;
                        write(
                            &mut self.writer,
                            sub_array.as_ref(),
                            &sub_nested,
                            type_.clone(),
                            length,
                            self.options.compression,
                            &mut self.scratch,
                        )
                        .unwrap();

                        let page_end = self.writer.offset;
                        PageMeta {
                            length: (page_end - page_start),
                            num_values: sub_array.len() as u64,
                        }
                    })
                    .collect();

                self.metas.push(ColumnMeta {
                    offset: start,
                    pages: page_metas,
                })
            }
        }

        Ok(())
    }
}
