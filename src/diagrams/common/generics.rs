fn tildes(text: &str) -> usize {
    text.bytes().filter(|&b| b == b'~').count()
}

// Pairs tildes from the outside in: the first with the last, the second with the one before it.
fn pair_tildes(text: &str) -> String {
    let count = tildes(text);
    if count <= 1 {
        return text.to_string();
    }
    let lead = count % 2 != 0 && text.starts_with('~');
    let mut chars: Vec<char> = text.chars().collect();
    let mut at = Vec::new();
    for i in usize::from(lead)..chars.len() {
        if chars[i] == '~' {
            at.push(i);
        }
    }
    let mut lo = 0usize;
    let mut hi = at.len().saturating_sub(1);
    while lo < hi {
        chars[at[lo]] = '<';
        chars[at[hi]] = '>';
        lo += 1;
        hi -= 1;
    }
    chars.into_iter().collect()
}

// Turns `List~int~` into `List<int>`, with the quirks of Mermaid's parseGenericTypes around commas.
pub fn parse_generic_types(input: &str) -> String {
    if !input.contains('~') {
        return input.to_string();
    }
    let mut sets: Vec<&str> = Vec::new();
    let mut start = 0;
    for (i, c) in input.char_indices() {
        if c == ',' {
            sets.push(&input[start..i]);
            sets.push(",");
            start = i + 1;
        }
    }
    sets.push(&input[start..]);

    let mut output: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sets.len() {
        let mut set = sets[i].to_string();
        if set == "," && i > 0 && i + 1 < sets.len() && tildes(sets[i - 1]) == 1 && tildes(sets[i + 1]) == 1 {
            set = format!("{},{}", sets[i - 1], sets[i + 1]);
            i += 1;
            output.pop();
        }
        output.push(pair_tildes(&set));
        i += 1;
    }
    output.concat()
}
