const NAMED: &str = "amp:38,lt:60,gt:62,quot:34,apos:39,nbsp:160,shy:173,iexcl:161,cent:162,pound:163,curren:164,yen:165,sect:167,\
copy:169,laquo:171,not:172,reg:174,deg:176,plusmn:177,sup2:178,sup3:179,micro:181,para:182,middot:183,\
sup1:185,raquo:187,frac14:188,frac12:189,frac34:190,iquest:191,times:215,divide:247,ndash:8211,mdash:8212,\
lsquo:8216,rsquo:8217,sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,\
hellip:8230,permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,euro:8364,trade:8482,larr:8592,\
uarr:8593,rarr:8594,darr:8595,harr:8596,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,forall:8704,\
part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,prod:8719,sum:8721,minus:8722,\
radic:8730,infin:8734,and:8743,or:8744,cap:8745,cup:8746,int:8747,there4:8756,sim:8764,cong:8773,\
asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,sub:8834,sup:8835,sube:8838,supe:8839,oplus:8853,otimes:8855,\
perp:8869,sdot:8901,loz:9674,spades:9824,clubs:9827,hearts:9829,diams:9830,check:10003,cross:10007,\
AMP:38,LT:60,GT:62,QUOT:34,lsqb:91,rsqb:93,lcub:123,rcub:125,verbar:124,VerticalLine:124,midast:42,\
DiacriticalGrave:96,Hat:94,UnderBar:95,\
equals:61,colon:58,semi:59,num:35,percnt:37,lpar:40,rpar:41,lbrack:91,rbrack:93,lbrace:123,rbrace:125,\
vert:124,sol:47,bsol:92,quest:63,excl:33,ast:42,plus:43,comma:44,period:46,dollar:36,commat:64,grave:96,\
hat:94,lowbar:95,tilde:126,Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,\
Iota:921,Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,\
Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,epsilon:949,zeta:950,\
eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,omicron:959,pi:960,rho:961,sigmaf:962,\
sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,omega:969";

fn lookup(name: &str) -> Option<char> {
    NAMED.split(',').find_map(|pair| {
        let (key, code) = pair.split_once(':')?;
        (key == name).then(|| char::from_u32(code.parse().ok()?)).flatten()
    })
}

fn code_point(n: u32) -> Option<char> {
    if n > 0 {
        char::from_u32(n)
    } else {
        None
    }
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

const OPEN: [char; 2] = ['ﬂ', '°'];

// Mermaid's placeholders run from `ﬂ°` to `¶ß`, with a second `°` marking a numeric code. Like
// Mermaid, turn them back into entities, so that each code is decoded once.
fn restore_placeholders(s: &[char]) -> Vec<char> {
    let find_open = |from: usize| (from..s.len().saturating_sub(1)).find(|&i| s[i] == OPEN[0] && s[i + 1] == OPEN[1]);
    let mut out = Vec::with_capacity(s.len());
    let mut last = 0;
    let mut from = find_open(0);
    while let Some(start) = from {
        let Some(stop) = (start + 2..s.len()).find(|&i| s[i] == '¶') else { break };
        if s.get(stop + 1) == Some(&'ß') {
            let numeric = s.get(start + 2) == Some(&'°');
            out.extend(&s[last..start]);
            out.push('&');
            if numeric {
                out.push('#');
            }
            out.extend(&s[start + if numeric { 3 } else { 2 }..stop]);
            out.push(';');
            last = stop + 2;
        }
        from = find_open(stop + 1);
    }
    out.extend(&s[last..]);
    out
}

// Shows placeholders as the `#code;` the author wrote, for error messages.
pub fn written_codes(text: &str) -> String {
    if !text.contains("¶ß") {
        return text.to_string();
    }
    let restored: String = restore_placeholders(&text.chars().collect::<Vec<_>>()).into_iter().collect();
    // restore_placeholders() writes `&code;` and `&#code;`; the author wrote `#code;`.
    let mut out = String::with_capacity(restored.len());
    let s: Vec<char> = restored.chars().collect();
    let mut i = 0;
    while i < s.len() {
        if s[i] == '&' {
            let mut j = i + 1;
            if s.get(j) == Some(&'#') {
                j += 1;
            }
            let start = j;
            while j < s.len() && is_word(s[j]) {
                j += 1;
            }
            if j > start && s.get(j) == Some(&';') {
                out.push('#');
                out.extend(&s[start..=j]);
                i = j + 1;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

// Turns Mermaid's `#35;` / `#quot;` codes (as hidden by encode_entities) and HTML entities into characters.
pub fn decode_entities(text: &str) -> String {
    let placeholders = text.contains("¶ß");
    if !placeholders && !text.contains('&') {
        return text.to_string();
    }
    let mut s: Vec<char> = text.chars().collect();
    if placeholders {
        s = restore_placeholders(&s);
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == '&' {
            let mut j = i + 1;
            let mut decoded = None;
            if s.get(j) == Some(&'#') {
                j += 1;
                let hex = matches!(s.get(j), Some('x') | Some('X'));
                if hex {
                    j += 1;
                }
                let start = j;
                while j < s.len() && (if hex { s[j].is_ascii_hexdigit() } else { s[j].is_ascii_digit() }) {
                    j += 1;
                }
                if j > start && s.get(j) == Some(&';') {
                    let digits: String = s[start..j].iter().collect();
                    // An out-of-range code is left as written.
                    decoded = Some(
                        u32::from_str_radix(&digits, if hex { 16 } else { 10 })
                            .ok()
                            .and_then(code_point)
                            .map(String::from)
                            .unwrap_or_else(|| s[i..=j].iter().collect()),
                    );
                }
            } else {
                let start = j;
                while j < s.len() && is_word(s[j]) {
                    j += 1;
                }
                if j > start && s.get(j) == Some(&';') {
                    let name: String = s[start..j].iter().collect();
                    decoded = Some(lookup(&name).map(String::from).unwrap_or_else(|| s[i..=j].iter().collect()));
                }
            }
            if let Some(text) = decoded {
                out.push_str(&text);
                i = j + 1;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes() {
        assert_eq!(decode_entities("ﬂ°°35¶ß ﬂ°quot¶ß &amp; &#x41; &nope;"), "# \" & A &nope;");
        assert_eq!(written_codes("ﬂ°°35¶ß ﬂ°quot¶ß"), "#35; #quot;");
    }
}
