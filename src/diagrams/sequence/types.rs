use crate::value::Value;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

// Insertion-ordered map matching JavaScript's Map semantics (preserving insertion slot on overwrite
// and tombstoning on delete).
#[derive(Debug, Clone)]
pub struct OrdMap<K, V> {
    entries: Vec<Option<(K, V)>>,
    index: HashMap<K, usize>,
}

impl<K, V> Default for OrdMap<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone, V> OrdMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.index.contains_key(key)
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let &idx = self.index.get(key)?;
        self.entries.get(idx).and_then(|slot| slot.as_ref().map(|(_, v)| v))
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let &idx = self.index.get(key)?;
        self.entries
            .get_mut(idx)
            .and_then(|slot| slot.as_mut().map(|(_, v)| v))
    }

    pub fn insert(&mut self, key: K, value: V) {
        if let Some(&idx) = self.index.get(&key) {
            if let Some(slot) = &mut self.entries[idx] {
                slot.1 = value;
                return;
            }
        }
        let idx = self.entries.len();
        self.entries.push(Some((key.clone(), value)));
        self.index.insert(key, idx);
    }

    #[allow(dead_code)]
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let idx = self.index.remove(key)?;
        self.entries[idx].take().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries
            .iter()
            .filter_map(|slot| slot.as_ref().map(|(k, v)| (k, v)))
    }
}

// The message kinds, numbered as in Mermaid.
#[allow(dead_code)]
pub mod linetype {
    pub const SOLID: i32 = 0;
    pub const DOTTED: i32 = 1;
    pub const NOTE: i32 = 2;
    pub const SOLID_CROSS: i32 = 3;
    pub const DOTTED_CROSS: i32 = 4;
    pub const SOLID_OPEN: i32 = 5;
    pub const DOTTED_OPEN: i32 = 6;
    pub const LOOP_START: i32 = 10;
    pub const LOOP_END: i32 = 11;
    pub const ALT_START: i32 = 12;
    pub const ALT_ELSE: i32 = 13;
    pub const ALT_END: i32 = 14;
    pub const OPT_START: i32 = 15;
    pub const OPT_END: i32 = 16;
    pub const ACTIVE_START: i32 = 17;
    pub const ACTIVE_END: i32 = 18;
    pub const PAR_START: i32 = 19;
    pub const PAR_AND: i32 = 20;
    pub const PAR_END: i32 = 21;
    pub const RECT_START: i32 = 22;
    pub const RECT_END: i32 = 23;
    pub const SOLID_POINT: i32 = 24;
    pub const DOTTED_POINT: i32 = 25;
    pub const AUTONUMBER: i32 = 26;
    pub const CRITICAL_START: i32 = 27;
    pub const CRITICAL_OPTION: i32 = 28;
    pub const CRITICAL_END: i32 = 29;
    pub const BREAK_START: i32 = 30;
    pub const BREAK_END: i32 = 31;
    pub const PAR_OVER_START: i32 = 32;
    pub const BIDIRECTIONAL_SOLID: i32 = 33;
    pub const BIDIRECTIONAL_DOTTED: i32 = 34;
    pub const SOLID_TOP: i32 = 41;
    pub const SOLID_BOTTOM: i32 = 42;
    pub const STICK_TOP: i32 = 43;
    pub const STICK_BOTTOM: i32 = 44;
    pub const SOLID_ARROW_TOP_REVERSE: i32 = 45;
    pub const SOLID_ARROW_BOTTOM_REVERSE: i32 = 46;
    pub const STICK_ARROW_TOP_REVERSE: i32 = 47;
    pub const STICK_ARROW_BOTTOM_REVERSE: i32 = 48;
    pub const SOLID_TOP_DOTTED: i32 = 51;
    pub const SOLID_BOTTOM_DOTTED: i32 = 52;
    pub const STICK_TOP_DOTTED: i32 = 53;
    pub const STICK_BOTTOM_DOTTED: i32 = 54;
    pub const SOLID_ARROW_TOP_REVERSE_DOTTED: i32 = 55;
    pub const SOLID_ARROW_BOTTOM_REVERSE_DOTTED: i32 = 56;
    pub const STICK_ARROW_TOP_REVERSE_DOTTED: i32 = 57;
    pub const STICK_ARROW_BOTTOM_REVERSE_DOTTED: i32 = 58;
    pub const CENTRAL_CONNECTION: i32 = 59;
    pub const CENTRAL_CONNECTION_REVERSE: i32 = 60;
    pub const CENTRAL_CONNECTION_DUAL: i32 = 61;
}

pub mod placement {
    pub const LEFTOF: i32 = 0;
    pub const RIGHTOF: i32 = 1;
    pub const OVER: i32 = 2;
}

#[derive(Debug, Clone)]
pub struct SeqText {
    pub text: String,
    pub wrap: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct SeqBoxData {
    pub text: Option<String>,
    pub color: String,
    pub wrap: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct SeqBox {
    pub name: Option<String>,
    pub wrap: bool,
    pub fill: String,
    pub actor_keys: Vec<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SeqActor {
    pub box_idx: Option<usize>,
    pub name: String,
    pub description: String,
    pub wrap: bool,
    pub prev_actor: Option<String>,
    pub next_actor: Option<String>,
    pub links: OrdMap<String, Value>,
    pub properties: OrdMap<String, Value>,
    // The id of the page element named by a `details` statement. Pele has no page to read it from.
    pub details: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct SeqNumbering {
    pub start: Option<f64>,
    pub step: Option<f64>,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub enum SeqMessageBody {
    Text(String),
    Numbering(SeqNumbering),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SeqMessage {
    pub id: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub message: SeqMessageBody,
    pub wrap: bool,
    pub kind: Option<i32>,
    pub activate: Option<bool>,
    pub placement: Option<i32>,
    pub central_connection: Option<i32>,
}

#[derive(Debug, Clone)]
pub enum SeqNoteActor {
    Single(String),
    Pair([String; 2]),
}

impl SeqNoteActor {
    pub fn as_single(&self) -> Option<&str> {
        match self {
            SeqNoteActor::Single(s) => Some(s),
            SeqNoteActor::Pair(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SeqNote {
    pub actor: SeqNoteActor,
    pub placement: i32,
    pub message: String,
    pub wrap: bool,
}

// One entry of the statement list the parser hands to `apply`, as Mermaid's grammar builds it.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct SeqStatement {
    pub kind: &'static str,
    pub actor: Option<SeqNoteActor>,
    pub draw: Option<String>,
    pub description: Option<SeqText>,
    pub config: Option<String>,
    pub signal_type: Option<i32>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub msg: Option<SeqText>,
    pub activate: Option<bool>,
    pub central_connection: Option<i32>,
    pub placement: Option<i32>,
    pub text: Option<SeqText>,
    pub box_data: Option<SeqBoxData>,
    pub box_text: Option<String>,
    pub loop_text: Option<SeqText>,
    pub opt_text: Option<SeqText>,
    pub alt_text: Option<SeqText>,
    pub par_text: Option<SeqText>,
    pub critical_text: Option<SeqText>,
    pub option_text: Option<SeqText>,
    pub break_text: Option<SeqText>,
    pub color: Option<SeqText>,
    pub sequence_index: Option<f64>,
    pub sequence_index_step: Option<f64>,
    pub sequence_visible: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum SeqItem {
    Stmt(Box<SeqStatement>),
    Str(String),
    List(Vec<SeqItem>),
}
