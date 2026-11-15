// Whether `word`, written in lower case, is at `at`. Only ASCII letters match in either case,
// as with the `i` flag on a regular expression without the `u` flag.
pub fn word_at(src: &[char], at: usize, word: &str) -> bool {
    for (k, w) in word.chars().enumerate() {
        let Some(&c) = src.get(at + k) else {
            return false;
        };
        let wc = w as u32;
        let cc = c as u32;
        if cc != wc && !(wc >= 97 && wc <= 122 && cc == wc - 32) {
            return false;
        }
    }
    true
}
