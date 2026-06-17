use crate::bridge::ffi;
use crate::item::{self, EMPTY_SUBASSEMBLY_ID};
use crate::opcode;
use crate::wire;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

pub fn deduplicate(items: &mut Vec<ffi::WireAssemblyItem>) -> BTreeMap<u64, u64> {
    let push_self = push_self();
    let tag_self = {
        let mut tag = push_self.clone();
        tag.kind = wire::KIND_TAG;
        tag
    };

    if items
        .iter()
        .any(|item| item::item_eq(item, &push_self) || item::item_eq(item, &tag_self))
    {
        return BTreeMap::new();
    }

    let mut replaced_tags = BTreeMap::new();
    let mut iterations = 0usize;

    loop {
        let mut blocks_seen = HashMap::<u64, Vec<usize>>::new();
        for index in 0..items.len() {
            if items[index].kind != wire::KIND_TAG {
                continue;
            }

            let fingerprint = block_fingerprint(items, index, &push_self);
            let seen_bucket = blocks_seen.entry(fingerprint).or_default();
            if let Some(seen_index) = seen_bucket.iter().copied().find(|seen_index| {
                compare_blocks(items, index, *seen_index, &push_self) == Ordering::Equal
            }) {
                if let (Some(from), Some(to)) = (
                    item::tag_value(&items[index]),
                    item::tag_value(&items[seen_index]),
                ) {
                    replaced_tags.insert(from, to);
                }
            } else {
                seen_bucket.push(index);
            }
        }

        if !apply_tag_replacement(items, &replaced_tags, EMPTY_SUBASSEMBLY_ID) {
            break;
        }
        iterations += 1;
    }

    if iterations == 0 {
        BTreeMap::new()
    } else {
        replaced_tags
    }
}

pub fn apply_tag_replacement(
    items: &mut [ffi::WireAssemblyItem],
    replacements: &BTreeMap<u64, u64>,
    sub_id: u64,
) -> bool {
    let mut changed = false;
    for assembly_item in items {
        if assembly_item.kind != wire::KIND_PUSH_TAG {
            continue;
        }

        let split = item::split_tag(assembly_item);
        if split.sub_id != sub_id {
            continue;
        }

        let mut replacement = replacements.get(&split.tag).copied();
        let mut current = replacement;
        while let Some(tag) = current {
            replacement = Some(tag);
            current = replacements.get(&tag).copied();
        }

        if let Some(tag) = replacement {
            changed = true;
            item::set_push_tag_sub_id_and_tag(assembly_item, sub_id, tag);
        }
    }
    changed
}

fn block_fingerprint(
    items: &[ffi::WireAssemblyItem],
    index: usize,
    push_self: &ffi::WireAssemblyItem,
) -> u64 {
    let tag_storage = items
        .get(index)
        .filter(|item| item.kind == wire::KIND_TAG)
        .map(|tag| {
            let mut push_tag = tag.clone();
            push_tag.kind = wire::KIND_PUSH_TAG;
            push_tag
        });
    let tag = tag_storage.as_ref().unwrap_or(push_self);

    let mut hasher = DefaultHasher::new();
    let mut block = BlockIter::new(items, index, Some(tag), Some(push_self));

    if block.peek().is_some_and(|item| item.kind == wire::KIND_TAG) {
        block.advance();
    }

    while let Some(item) = block.next_item() {
        hash_item(item, &mut hasher);
    }

    hasher.finish()
}

fn hash_item(item: &ffi::WireAssemblyItem, hasher: &mut impl Hasher) {
    item.kind.hash(hasher);
    match item.kind {
        wire::KIND_OPERATION => item.opcode.hash(hasher),
        wire::KIND_VERBATIM_BYTECODE => {
            item.verbatim_arguments.hash(hasher);
            item.verbatim_return_values.hash(hasher);
            item.verbatim_data.hash(hasher);
        }
        _ => item.data.hash(hasher),
    }
}

fn compare_blocks(
    items: &[ffi::WireAssemblyItem],
    first_index: usize,
    second_index: usize,
    push_self: &ffi::WireAssemblyItem,
) -> Ordering {
    if first_index == second_index {
        return Ordering::Equal;
    }

    let first_tag_storage = items
        .get(first_index)
        .filter(|item| item.kind == wire::KIND_TAG)
        .map(|tag| {
            let mut push_tag = tag.clone();
            push_tag.kind = wire::KIND_PUSH_TAG;
            push_tag
        });
    let first_tag = first_tag_storage.as_ref().unwrap_or(push_self);

    let second_tag_storage = items
        .get(second_index)
        .filter(|item| item.kind == wire::KIND_TAG)
        .map(|tag| {
            let mut push_tag = tag.clone();
            push_tag.kind = wire::KIND_PUSH_TAG;
            push_tag
        });
    let second_tag = second_tag_storage.as_ref().unwrap_or(push_self);

    let mut first = BlockIter::new(items, first_index, Some(first_tag), Some(push_self));
    let mut second = BlockIter::new(items, second_index, Some(second_tag), Some(push_self));

    if first.peek().is_some_and(|item| item.kind == wire::KIND_TAG) {
        first.advance();
    }
    if second
        .peek()
        .is_some_and(|item| item.kind == wire::KIND_TAG)
    {
        second.advance();
    }

    loop {
        match (first.next_item(), second.next_item()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) => match item::item_cmp(a, b) {
                Ordering::Equal => {}
                ordering => return ordering,
            },
        }
    }
}

struct BlockIter<'a, 'b> {
    items: &'a [ffi::WireAssemblyItem],
    index: usize,
    replace_item: Option<&'b ffi::WireAssemblyItem>,
    replace_with: Option<&'b ffi::WireAssemblyItem>,
}

impl<'a, 'b> BlockIter<'a, 'b> {
    fn new(
        items: &'a [ffi::WireAssemblyItem],
        index: usize,
        replace_item: Option<&'b ffi::WireAssemblyItem>,
        replace_with: Option<&'b ffi::WireAssemblyItem>,
    ) -> Self {
        Self {
            items,
            index,
            replace_item,
            replace_with,
        }
    }

    fn peek(&self) -> Option<&ffi::WireAssemblyItem> {
        self.items.get(self.index)
    }

    fn advance(&mut self) {
        if self.index >= self.items.len() {
            return;
        }
        if self.items[self.index].kind == wire::KIND_OPERATION
            && opcode::alters_control_flow(self.items[self.index].opcode)
            && !item::is_operation(&self.items[self.index], opcode::op::JUMPI)
        {
            self.index = self.items.len();
            return;
        }
        self.index += 1;
        while self
            .items
            .get(self.index)
            .is_some_and(|item| item.kind == wire::KIND_TAG)
        {
            self.index += 1;
        }
    }

    fn next_item(&mut self) -> Option<&ffi::WireAssemblyItem> {
        let index = self.index;
        let raw = self.items.get(index)?;
        let replace = self
            .replace_item
            .zip(self.replace_with)
            .is_some_and(|(replace_item, _)| item::item_eq(raw, replace_item));

        self.advance();

        if replace {
            self.replace_with
        } else {
            Some(&self.items[index])
        }
    }
}

fn push_self() -> ffi::WireAssemblyItem {
    ffi::WireAssemblyItem {
        kind: wire::KIND_PUSH_TAG,
        opcode: 0,
        data: item::u256_from_neg_u64(4),
        verbatim_data: Vec::new(),
        verbatim_arguments: 0,
        verbatim_return_values: 0,
        jump_type: wire::JUMP_ORDINARY,
        modifier_depth: 0,
        debug_data_id: 0,
        has_pushed_value: false,
        pushed_value: Vec::new(),
        has_immutable_occurrences: false,
        immutable_occurrences: 0,
    }
}
