//! Prints the system's word boundaries for sample text and how long they take.
fn main() {
    let start = std::time::Instant::now();
    let available = aegle_segment::available();
    println!("available: {available} (first call {:?})", start.elapsed());
    if !available {
        return;
    }
    let text = "中文分词测试，这是一个较长的句子用来测量分词耗时。東京都に行きます。ภาษาไทยทดสอบ";
    for _ in 0..3 {
        let _ = aegle_segment::boundaries(text);
    }
    let start = std::time::Instant::now();
    let rounds = 100;
    for _ in 0..rounds {
        let bounds = aegle_segment::boundaries(text).unwrap();
        assert!(bounds.starts().len() > 2);
    }
    println!(
        "{rounds} segmentations of {} bytes: {:?} each",
        text.len(),
        start.elapsed() / rounds
    );
    for sample in ["中文分词测试", "one two three"] {
        let bounds = aegle_segment::boundaries(sample).unwrap();
        let words: Vec<&str> = bounds
            .starts()
            .windows(2)
            .map(|pair| &sample[pair[0]..pair[1]])
            .collect();
        println!("{sample:?} => {words:?}");
    }
}
