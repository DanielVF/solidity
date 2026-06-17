use crate::bridge::ffi;
use crate::item::{self, EMPTY_SUBASSEMBLY_ID};
use crate::wire;
use std::collections::BTreeSet;

pub fn optimise(
    items: &mut Vec<ffi::WireAssemblyItem>,
    tags_referenced_from_outside: &[u64],
) -> bool {
    let mut references = referenced_tags(items, EMPTY_SUBASSEMBLY_ID);
    references.extend(tags_referenced_from_outside.iter().copied());

    let initial_size = items.len();
    items.retain(|assembly_item| {
        if assembly_item.kind != wire::KIND_TAG {
            return true;
        }
        let split = item::split_tag(assembly_item);
        debug_assert_eq!(split.sub_id, EMPTY_SUBASSEMBLY_ID);
        references.contains(&split.tag)
    });
    items.len() != initial_size
}

pub fn referenced_tags(items: &[ffi::WireAssemblyItem], sub_id: u64) -> BTreeSet<u64> {
    let mut out = BTreeSet::new();
    for assembly_item in items {
        if assembly_item.kind == wire::KIND_PUSH_TAG {
            let split = item::split_tag(assembly_item);
            if split.sub_id == sub_id {
                out.insert(split.tag);
            }
        }
    }
    out
}
