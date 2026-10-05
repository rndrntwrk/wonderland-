//! Bounded offline decoder: accepts normalized metadata supplied by B; no header parser.
#![forbid(unsafe_code)]
use std::{fs::File,io::{Read,Write}};
use wonderland_audio_runtime::{codec::*,AudioError,Result};
fn number<T:std::str::FromStr>(text:&str)->Result<T>{text.parse().map_err(|_|AudioError::Invalid("numeric metadata"))}
fn run()->Result<()>{
    let args:Vec<String>=std::env::args().collect();
    if args.len()!=10{return Err(AudioError::Invalid("usage: audio-decode xa-speech|xa-music|utk|pcm RATE CHANNELS WIDTH FRAMES OFFSET LENGTH INPUT OUTPUT"));}
    let encoding=match args[1].as_str(){"xa-speech"=>Encoding::XaSpeech,"xa-music"=>Encoding::XaMusic,"utk"=>Encoding::Utk,"pcm"=>Encoding::PcmWave,_=>return Err(AudioError::Unsupported("encoding"))};
    let metadata=SampleMetadata{encoding,sample_rate:number(&args[2])?,channels:number(&args[3])?,bits_per_sample:number(&args[4])?,frames:number(&args[5])?,payload_offset:number(&args[6])?,payload_bytes:number(&args[7])?};
    let limits=DecodeLimits::default();let input=File::open(&args[8]).map_err(|e|AudioError::Io(e.to_string()))?;
    if input.metadata().map_err(|e|AudioError::Io(e.to_string()))?.len()>limits.input_bytes as u64{return Err(AudioError::Limit("encoded audio bytes"));}
    let mut bytes=Vec::new();input.take(limits.input_bytes as u64+1).read_to_end(&mut bytes).map_err(|e|AudioError::Io(e.to_string()))?;
    let pcm=decode_sample(&bytes,&metadata,&limits)?;let wave=encode_wave(&pcm)?;
    let mut output=File::options().write(true).create_new(true).open(&args[9]).map_err(|e|AudioError::Io(e.to_string()))?;
    output.write_all(&wave).map_err(|e|AudioError::Io(e.to_string()))?;output.flush().map_err(|e|AudioError::Io(e.to_string()))?;
    println!("PCM16 {} Hz {} channels {} frames",pcm.sample_rate,pcm.channels,pcm.frames());Ok(())
}
fn main(){if let Err(error)=run(){eprintln!("{error}");std::process::exit(2);}}
