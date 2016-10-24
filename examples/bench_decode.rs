// Claxon -- A FLAC decoding library in Rust
// Copyright 2016 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate claxon;
extern crate time;

use claxon::FlacReader;
use claxon::sample::Sample;
use std::env;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use time::PreciseTime;

/// Reads a file into memory entirely.
fn read_file<P: AsRef<Path>>(path: P) -> Vec<u8> {
    let mut file = File::open(path).unwrap();
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    data
}

/// Decode a file into 16-bit or 32-bit integers.
///
/// For every block decoded, appends to the vector the average time spent per
/// sample for that block in nanoseconds.
///
/// Returns a pair of (total time / total samples, total bytes / total time) in
/// units (nanoseconds, bytes per second). Different channels account for
/// different samples; a stereo file of the same sample rate and duration as a
/// mono file will have twice as many samples. Bytes refers to the number of
/// input bytes. (TODO: Do not count header bytes there.)
fn decode_file<S: Sample>(data: &[u8],
                          sample_times_ns: &mut Vec<i32>)
                          -> (i64, i64) {
    let cursor = Cursor::new(data);
    let mut reader = FlacReader::new(cursor).unwrap();

    let bps = reader.streaminfo().bits_per_sample as u64;
    let num_samples = reader.streaminfo().samples.unwrap() as i64 * reader.streaminfo().channels as i64;
    assert!(bps < 8 * 16);

    let mut sample_buffer = Vec::new();
    let mut frame_reader = reader.blocks::<S>();
    let mut frame_epoch = PreciseTime::now();
    let epoch = frame_epoch;

    loop {
        match frame_reader.read_next_or_eof(sample_buffer) {
            Ok(Some(block)) => {
                // Update timing information.
                let now = PreciseTime::now();
                let duration_ns = frame_epoch.to(now).num_nanoseconds().unwrap();
                let num_samples = block.len() as i64 * block.channels() as i64;
                sample_times_ns.push(duration_ns as i32 / num_samples as i32);
                frame_epoch = now;

                // Recycle the buffer for the next frame. There should be a
                // black box to prevent optimizing this away, but it is only
                // available on unstable Rust, and it seems that nothing is
                // optimized away here anyway. The decode needs to happen anyway
                // to decide whether we hit the panic below.
                sample_buffer = block.into_buffer();
            },
            Ok(None) => break, // End of file.
            Err(_) => panic!("failed to decode")
        }
    }

    let total_duration_ns = epoch.to(PreciseTime::now()).num_nanoseconds().unwrap();
    let ns_per_sample = total_duration_ns / num_samples;
    let bytes_per_sec = data.len() as i64 * 1000_000_000 / total_duration_ns;
    (ns_per_sample, bytes_per_sec)
}

fn print_stats(sample_times_ns: &mut Vec<i32>, stats_pair: (i64, i64)) {
    let (ns_per_sample, bytes_per_sec) = stats_pair;
    sample_times_ns.sort();

    let p10 = sample_times_ns[10 * sample_times_ns.len() / 100];
    let p50 = sample_times_ns[50 * sample_times_ns.len() / 100];
    let p90 = sample_times_ns[90 * sample_times_ns.len() / 100];

    println!("{} {} {} {} {}", p10, p50, p90, ns_per_sample, bytes_per_sec);
}

fn main() {
    let bits = env::args().nth(1).expect("no bit depth given");
    let fname = env::args().nth(2).expect("no file given");

    let data = read_file(fname);
    let mut sample_times_ns = Vec::new();

    if bits == "16" {
        // TODO: Do several passes and report timing information.
        let stats_pair = decode_file::<i16>(&data, &mut sample_times_ns);
        print_stats(&mut sample_times_ns, stats_pair);
        sample_times_ns.clear();
    } else if bits == "32" {
        let stats_pair = decode_file::<i32>(&data, &mut sample_times_ns);
        print_stats(&mut sample_times_ns, stats_pair);
        sample_times_ns.clear();
    } else {
        panic!("expected bit depth of 16 or 32");
    }
}
