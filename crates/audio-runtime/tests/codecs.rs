//! All payloads here are synthetic and redistributable, not game audio.
use wonderland_audio_runtime::{codec::*,AudioError};
fn meta(encoding:Encoding,channels:u16,frames:u64,len:usize)->SampleMetadata{SampleMetadata{encoding,sample_rate:22050,channels,frames,bits_per_sample:16,payload_offset:0,payload_bytes:len as u64}}
#[test]
fn xa_high_low_nibbles_stereo_order_and_channel_history(){
    let mut mono=vec![0,0x1f];mono.resize(15,0);let pcm=decode_sample(&mono,&meta(Encoding::XaSpeech,1,28,15),&DecodeLimits::default()).unwrap();assert_eq!(&pcm.samples[..4],&[4096,-4096,0,0]);
    let mut stereo=vec![0,0,0x12,0xf8];stereo.resize(30,0);let pcm=decode_sample(&stereo,&meta(Encoding::XaMusic,2,28,30),&DecodeLimits::default()).unwrap();assert_eq!(&pcm.samples[..4],&[4096,-4096,8192,-32768]);
    mono[14]=0x12;mono.push(0x10);mono.resize(30,0);let pcm=decode_sample(&mono,&meta(Encoding::XaSpeech,1,56,30),&DecodeLimits::default()).unwrap();assert_eq!(&pcm.samples[26..30],&[4096,8192,7680,7200]);
}
#[test]
fn xa_all_predictors_are_valid_but_incomplete_blocks_and_capacity_mismatch_fail(){
    for predictor in 0..16 {let mut bytes=vec![predictor<<4];bytes.resize(15,0);assert!(decode_sample(&bytes,&meta(Encoding::XaSpeech,1,28,15),&DecodeLimits::default()).is_ok());}
    let bytes=vec![0;14];assert!(decode_sample(&bytes,&meta(Encoding::XaSpeech,1,28,14),&DecodeLimits::default()).is_err());
    assert!(decode_sample(&[0;15],&meta(Encoding::XaSpeech,1,27,15),&DecodeLimits::default()).is_err());
}
#[test]
fn normalized_pcm_integer_widths_decode_and_wave_output_retains_frames(){
    let mut m=meta(Encoding::PcmWave,1,3,3);m.bits_per_sample=8;let pcm=decode_sample(&[0,128,255],&m,&DecodeLimits::default()).unwrap();assert_eq!(pcm.samples,vec![-32768,0,32512]);
    let mut m=meta(Encoding::PcmWave,1,3,9);m.bits_per_sample=24;let pcm=decode_sample(&[0,0,128,255,255,127,0,1,0],&m,&DecodeLimits::default()).unwrap();assert_eq!(pcm.samples,vec![-32768,32767,1]);
    let wav=encode_wave(&pcm).unwrap();assert_eq!(&wav[..4],b"RIFF");assert_eq!(&wav[40..44],&6u32.to_le_bytes());assert_eq!(&wav[44..],&[0,128,255,127,1,0]);
}
#[test]
fn invalid_metadata_and_mp3_never_report_decode_success(){
    let mut m=meta(Encoding::Utk,1,u64::MAX,8);assert!(decode_sample(&[0;8],&m,&DecodeLimits::default()).is_err());m.frames=1;m.payload_offset=u64::MAX;assert!(decode_sample(&[0;8],&m,&DecodeLimits::default()).is_err());
    assert_eq!(decode_sample(&[0;8],&meta(Encoding::Mp3,1,1,8),&DecodeLimits::default()),Err(AudioError::Unsupported("MP3 requires external codec")));
    let mut m=meta(Encoding::PcmWave,3,1,6);assert!(decode_sample(&[0;6],&m,&DecodeLimits::default()).is_err());m.channels=1;m.sample_rate=0;assert!(decode_sample(&[0;6],&m,&DecodeLimits::default()).is_err());
}
struct Bits {data:Vec<u8>,n:usize}
impl Bits {fn new()->Self{Self{data:vec![],n:0}}fn put(&mut self,x:u32,n:usize){for i in 0..n{if self.n%8==0{self.data.push(0);}let last=self.data.len()-1;self.data[last]|=(((x>>i)&1) as u8)<<(self.n%8);self.n+=1;}}}
fn utk(voiced:bool,half:Option<(u32,u32)>,frames:usize)->Vec<u8>{
    let mut b=Bits::new();b.put(u32::from(half.is_some()),1);b.put(0,4);b.put(0,4);b.put(0,6);
    for frame in 0..frames {for i in 0..12{b.put(if i<4{if i==0 && voiced{0}else{32}}else{16},if i<4{6}else{5});}
        for sub in 0..4{b.put(if frame>0{216}else{0},8);b.put(if frame>0{5}else{0},4);b.put(0,6);if let Some((a,z))=half{b.put(a,1);b.put(z,1);}let count=if half.is_some(){54}else{108};
            for i in 0..count{let v=(i+sub)%3;if voiced{b.put(match v{0=>1,1=>2,_=>0},2);}else{match v{0=>b.put(0,1),1=>{b.put(1,1);b.put(0,1);},_=>{b.put(1,1);b.put(1,1);}}}}
        }
    }b.data.push(0);b.data
}
#[test]
fn utk_unvoiced_and_voiced_integer_magnitude_have_source_samples(){
    for (voiced,want) in [(false,vec![0,-16,16,0,-16,16]),(true,vec![8,-8,0,8,-8,0])]{let bytes=utk(voiced,None,1);let pcm=decode_sample(&bytes,&meta(Encoding::Utk,1,432,bytes.len()),&DecodeLimits::default()).unwrap();assert_eq!(&pcm.samples[..6],want);assert_eq!(pcm.samples.len(),432);}
}
#[test]
fn utk_half_excitation_zero_fill_and_sinc_produce_real_samples(){
    let b=utk(true,Some((0,1)),1);let p=decode_sample(&b,&meta(Encoding::Utk,1,432,b.len()),&DecodeLimits::default()).unwrap();assert_eq!(&p.samples[..8],&[8,0,-8,0,0,0,8,0]);
    let b=utk(true,Some((0,0)),1);let p=decode_sample(&b,&meta(Encoding::Utk,1,432,b.len()),&DecodeLimits::default()).unwrap();assert_eq!(&p.samples[..8],&[4,0,-4,-3,0,3,4,0]);
}
#[test]
fn utk_partial_last_frame_and_frame_history_preserve_declared_length(){
    let b=utk(false,None,2);let p=decode_sample(&b,&meta(Encoding::Utk,1,450,b.len()),&DecodeLimits::default()).unwrap();assert_eq!(p.samples.len(),450);assert!(p.samples[432..].iter().any(|x|*x!=0));
}
#[test]
fn utk_exhaustion_and_unary_budget_fail_in_finite_work(){
    assert!(matches!(decode_sample(&[0;2],&meta(Encoding::Utk,1,432,2),&DecodeLimits::default()),Err(AudioError::Truncated(_))));let b=utk(false,None,1);for length in [2,5,10,b.len()/2]{assert!(decode_sample(&b[..length],&meta(Encoding::Utk,1,432,length),&DecodeLimits::default()).is_err());}
}
