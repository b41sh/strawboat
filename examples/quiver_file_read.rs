use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::time::Instant;

use arrow::chunk::Chunk;

use arrow::error::Result;

use quiver::read::reader::{infer_schema, read_meta, QuiverReader};
use quiver::ColumnMeta;

/// Simplest way: read all record batches from the file. This can be used e.g. for random access.
// cargo run --package quiver --example quiver_file_read  --release /tmp/input.quiver
fn main() -> Result<()> {
    use std::env;
    let args: Vec<String> = env::args().collect();

    let file_path = &args[1];

    let t = Instant::now();
    {
        let mut reader = File::open(file_path).unwrap();
        // we can read its metadata:
        // and infer a [`Schema`] from the `metadata`.
        let schema = infer_schema(&mut reader).unwrap();

        let metas: Vec<ColumnMeta> = read_meta(&mut reader)?;

        let mut readers = vec![];
        for (meta, field) in metas.iter().zip(schema.fields.iter()) {
            let mut reader = File::open(file_path).unwrap();
            reader.seek(std::io::SeekFrom::Start(meta.offset)).unwrap();
            let reader = reader.take(meta.total_len());

            let buffer_size = meta.total_len().min(8192) as usize;
            let reader = BufReader::with_capacity(buffer_size, reader);
            let scratch = Vec::with_capacity(8 * 1024);

            let pa_reader = QuiverReader::new(
                reader,
                field.data_type().clone(),
                meta.pages.clone(),
                scratch,
            );

            readers.push(pa_reader);
        }

        'FOR: loop {
            let mut chunks = Vec::new();
            for reader in readers.iter_mut() {
                if !reader.has_next() {
                    break 'FOR;
                }
                chunks.push(reader.next_array().unwrap());
            }

            let chunk = Chunk::new(chunks);
            println!("READ -> {:?} rows", chunk.len());
        }
    }

    println!("cost {:?} ms", t.elapsed().as_millis());
    // println!("{}", print::write(&[chunks], &["names", "tt", "3", "44"]));
    Ok(())
}