use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct CharStream<'a> {
    source: Cow<'a, [u8]>,
    position: usize,
}

impl Default for CharStream<'static> {
    fn default() -> Self {
        Self {
            source: Cow::Owned(Vec::new()),
            position: 0,
        }
    }
}

impl CharStream<'static> {
    pub fn new(source: Vec<u8>) -> Self {
        Self {
            source: Cow::Owned(source),
            position: 0,
        }
    }
}

impl<'a> CharStream<'a> {
    pub fn borrowed(source: &'a [u8]) -> Self {
        Self {
            source: Cow::Borrowed(source),
            position: 0,
        }
    }

    pub fn source(&self) -> &[u8] {
        &self.source
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn is_past_end_of_input(&self, chars_forward: usize) -> bool {
        self.position.saturating_add(chars_forward) >= self.source.len()
    }

    pub fn get(&self, chars_forward: usize) -> u8 {
        self.source
            .get(self.position.saturating_add(chars_forward))
            .copied()
            .unwrap_or(0)
    }

    pub fn advance_and_get(&mut self, chars: usize) -> u8 {
        if self.is_past_end_of_input(0) {
            return 0;
        }
        self.position = self.position.saturating_add(chars);
        if self.is_past_end_of_input(0) {
            return 0;
        }
        self.get(0)
    }

    pub fn rollback(&mut self, amount: usize) -> u8 {
        assert!(self.position >= amount);
        self.position -= amount;
        self.get(0)
    }

    pub fn set_position(&mut self, location: usize) -> u8 {
        assert!(
            location <= self.source.len(),
            "Attempting to set position past end of source."
        );
        self.position = location;
        self.get(0)
    }

    pub fn reset(&mut self) {
        self.position = 0;
    }

    pub fn prefix_match(&self, sequence: &[u8]) -> bool {
        if self.is_past_end_of_input(sequence.len()) {
            return false;
        }

        sequence
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte == self.get(index))
    }
}
