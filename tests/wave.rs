use bp::wav::{COARSE, FINE, Refusal, Wave};
use support::RATE;

const LOW_RATE: u32 = 8_000;
const TEN_MINUTES: usize = 600;
const PCM: u16 = 1;
const FLOAT: u16 = 3;
const HALF: i16 = 16_384;
const FULL: i16 = i16::MAX;
const STEP: f32 = 1.0 / 32_768.0;

fn header(rate: u32, channels: u16, bits: u16, format: u16, data: &[u8], leading: &[u8]) -> Vec<u8> {
    let block = channels * bits / 8;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(leading);
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&format.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
    bytes.extend_from_slice(&block.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(data);
    let riff = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&riff.to_le_bytes());
    bytes
}

fn sixteen(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    header(rate, channels, 16, PCM, &data, &[])
}

fn twenty_four(rate: u32, samples: &[i32]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()[..3].to_vec()).collect();
    header(rate, 1, 24, PCM, &data, &[])
}

fn floating(rate: u32, channels: u16, samples: &[f32]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    header(rate, channels, 32, FLOAT, &data, &[])
}

fn parsed(bytes: &[u8]) -> Wave {
    Wave::parse(String::from("test"), String::from("test.wav"), bytes).expect("wanted the file to read")
}

fn frame(wave: &Wave, index: usize) -> Option<(f32, f32)> {
    let at = index.checked_mul(wave.channels)?;
    let left = f32::from(*wave.pcm.get(at)?) * STEP;
    let right = if wave.channels == 2 { f32::from(*wave.pcm.get(at + 1)?) * STEP } else { left };
    Some((left, right))
}

#[test]
fn sixteen_bit_stereo_frames_read_back_to_the_word() {
    let samples: Vec<i16> = (0..200).flat_map(|frame| [frame * 100, -frame * 50]).collect();
    let wave = parsed(&sixteen(RATE, 2, &samples));
    assert_eq!((wave.frames(), wave.channels, wave.rate), (200, 2, f64::from(RATE)), "wanted 200 stereo frames at {RATE}, got {} frames, {} channels at {}", wave.frames(), wave.channels, wave.rate);
    for index in 0..200usize {
        let (left, right) = frame(&wave, index).expect("wanted a frame inside the file");
        let (wanted_left, wanted_right) = (index as f32 * 100.0 * STEP, -(index as f32) * 50.0 * STEP);
        assert!((left - wanted_left).abs() < STEP / 2.0 && (right - wanted_right).abs() < STEP / 2.0, "frame {index}: wanted ({wanted_left}, {wanted_right}), got ({left}, {right})");
    }
    assert_eq!(frame(&wave, 200), None, "wanted nothing past the last frame, got {:?}", frame(&wave, 200));
}

#[test]
fn twenty_four_bit_mono_and_float_stereo_read_too() {
    let mono = parsed(&twenty_four(RATE, &[0, 4_194_304, -8_388_608, 8_388_607]));
    assert_eq!((mono.frames(), mono.channels), (4, 1), "wanted four mono frames, got {} frames in {} channels", mono.frames(), mono.channels);
    let (left, right) = frame(&mono, 1).expect("wanted the second frame");
    assert!((left - 0.5).abs() < STEP && left == right, "wanted the 24-bit half-scale frame at 0.5 in both ears, got ({left}, {right})");
    let (left, _) = frame(&mono, 2).expect("wanted the third frame");
    assert!((left + 1.0).abs() < STEP, "wanted the 24-bit floor at -1.0, got {left}");
    let float = parsed(&floating(RATE, 2, &[0.25, -0.25, 2.0, -2.0]));
    let (left, right) = frame(&float, 0).expect("wanted the first float frame");
    assert!((left - 0.25).abs() < STEP && (right + 0.25).abs() < STEP, "wanted (0.25, -0.25) from the float file, got ({left}, {right})");
    let (left, right) = frame(&float, 1).expect("wanted the second float frame");
    assert!(left <= 1.0 && right >= -1.0 && left > 0.99 && right < -0.99, "wanted a float past full scale held at the rails, got ({left}, {right})");
}

#[test]
fn chunks_before_the_format_are_stepped_over() {
    let mut list = Vec::new();
    list.extend_from_slice(b"LIST");
    list.extend_from_slice(&5u32.to_le_bytes());
    list.extend_from_slice(b"INFOx");
    list.push(0);
    let data: Vec<u8> = [HALF, HALF, HALF].iter().flat_map(|sample| sample.to_le_bytes()).collect();
    let wave = parsed(&header(RATE, 1, 16, PCM, &data, &list));
    assert_eq!(wave.frames(), 3, "wanted the three frames after an odd-sized LIST chunk, got {}", wave.frames());
    let (left, _) = frame(&wave, 2).expect("wanted the last frame");
    assert!((left - 0.5).abs() < STEP, "wanted 0.5 in the last frame, got {left}");
}

#[test]
fn what_is_not_a_wave_and_what_runs_past_ten_minutes_are_refused() {
    assert_eq!(Wave::parse(String::from("x"), String::from("x.wav"), b"not a riff at all").err(), Some(Refusal::Unreadable), "wanted plain text refused as unreadable");
    assert_eq!(Wave::parse(String::from("x"), String::from("x.wav"), &header(RATE, 1, 12, PCM, &[0; 6], &[])).err(), Some(Refusal::Unreadable), "wanted a twelve-bit file refused as unreadable");
    let seconds = TEN_MINUTES * LOW_RATE as usize;
    let whole = header(LOW_RATE, 1, 8, PCM, &vec![128u8; seconds], &[]);
    assert_eq!(Wave::parse(String::from("x"), String::from("x.wav"), &whole).ok().map(|wave| wave.frames()), Some(seconds), "wanted exactly ten minutes read whole");
    let over = header(LOW_RATE, 1, 8, PCM, &vec![128u8; seconds + 1], &[]);
    assert_eq!(Wave::parse(String::from("x"), String::from("x.wav"), &over).err(), Some(Refusal::TooLong), "wanted one frame past ten minutes refused as too long");
    let missing = Wave::missing(String::from("gone"), Some(String::from("gone.wav")));
    assert_eq!((missing.frames(), frame(&missing, 0)), (0, None), "wanted a missing file to hold no frames");
}

#[test]
fn peaks_tell_silence_from_a_burst_on_both_rungs() {
    let mut samples = vec![0i16; 20_000];
    for sample in &mut samples[5_000..5_100] {
        *sample = FULL;
    }
    for (index, sample) in samples[12_300..12_364].iter_mut().enumerate() {
        *sample = if index % 2 == 0 { HALF } else { -HALF };
    }
    let wave = parsed(&sixteen(RATE, 1, &samples));
    assert_eq!((wave.fine.len(), wave.coarse.len()), (20_000usize.div_ceil(FINE), 20_000usize.div_ceil(COARSE)), "wanted one fine byte per {FINE} frames and one coarse per {COARSE}, got {} and {}", wave.fine.len(), wave.coarse.len());
    assert_eq!(wave.peak(0.0, 4_032.0), 0, "wanted silence before the burst on the fine rung");
    assert_eq!(wave.peak(5_000.0, 5_100.0), 255, "wanted the burst at full scale on the fine rung");
    assert_eq!(wave.peak(12_300.0, 12_364.0), 128, "wanted the half-scale wobble at 128, got {}", wave.peak(12_300.0, 12_364.0));
    assert_eq!(wave.peak(0.0, 20_000.0), 255, "wanted the whole file's coarse peak at full scale");
    assert_eq!(wave.peak(8_192.0, 12_288.0), 0, "wanted the coarse bucket between the burst and the wobble silent, got {}", wave.peak(8_192.0, 12_288.0));
    assert_eq!(wave.peak(12_288.0, 16_384.0), 128, "wanted the coarse bucket holding the wobble at 128, got {}", wave.peak(12_288.0, 16_384.0));
    assert_eq!(wave.peak(-500.0, 0.0), 0, "wanted nothing before the file's first frame");
    assert_eq!(wave.peak(20_000.0, 30_000.0), 0, "wanted nothing past the file's last frame");
}
