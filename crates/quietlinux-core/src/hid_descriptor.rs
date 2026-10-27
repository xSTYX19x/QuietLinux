//! Minimal HID report descriptor parsing (short items only). No I/O.
//!
//! Ported from `lightmount/hid.py` of `bbossa/keyboard_bequiet_lightmount_rgb`.

const TYPE_MAIN: u8 = 0;
const TYPE_GLOBAL: u8 = 1;
const TYPE_LOCAL: u8 = 2;
const TAG_COLLECTION: u8 = 0xa;
const TAG_USAGE_PAGE: u8 = 0x0; // global
const TAG_USAGE: u8 = 0x0; // local
const LONG_ITEM: u8 = 0xfe;

/// `(usage page, usage)` of a collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub page: u32,
    pub id: u32,
}

impl Usage {
    pub const fn new(page: u32, id: u32) -> Self {
        Self { page, id }
    }
}

/// A short item: `(type, tag, value)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Item {
    kind: u8,
    tag: u8,
    value: u32,
}

/// Short items of a descriptor. Long items are skipped; a truncated item ends the iteration.
fn items(descriptor: &[u8]) -> impl Iterator<Item = Item> + '_ {
    let mut i = 0;
    std::iter::from_fn(move || {
        loop {
            let &prefix = descriptor.get(i)?;
            if prefix == LONG_ITEM {
                // [fe][size][tag][data...]
                i += 3 + usize::from(*descriptor.get(i + 1)?);
                continue;
            }
            let size = [0, 1, 2, 4][usize::from(prefix & 3)];
            let bytes = descriptor.get(i + 1..i + 1 + size)?;
            let value = bytes
                .iter()
                .rev()
                .fold(0u32, |acc, &b| (acc << 8) | u32::from(b));
            i += 1 + size;
            return Some(Item {
                kind: (prefix >> 2) & 3,
                tag: prefix >> 4,
                value,
            });
        }
    })
}

/// Usage of the first collection, or `None`.
pub fn top_level_usage(descriptor: &[u8]) -> Option<Usage> {
    let mut page = None;
    let mut usage = None;
    for item in items(descriptor) {
        match (item.kind, item.tag) {
            (TYPE_GLOBAL, TAG_USAGE_PAGE) => page = Some(item.value),
            (TYPE_LOCAL, TAG_USAGE) => usage = Some(item.value),
            (TYPE_MAIN, TAG_COLLECTION) => return Some(Usage::new(page?, usage?)),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_collection() {
        // Usage Page (0xff00), Usage (0x01), Collection (Application)
        let descriptor = [0x06, 0x00, 0xff, 0x09, 0x01, 0xa1, 0x01, 0xc0];
        assert_eq!(top_level_usage(&descriptor), Some(Usage::new(0xff00, 0x01)));
    }

    #[test]
    fn long_item_skipped() {
        let descriptor = [0xfe, 0x01, 0x00, 0xaa, 0x05, 0x59, 0x09, 0x01, 0xa1, 0x01];
        assert_eq!(top_level_usage(&descriptor), Some(Usage::new(0x59, 0x01)));
    }

    #[test]
    fn malformed() {
        assert_eq!(top_level_usage(&[]), None);
        assert_eq!(top_level_usage(&[0x06, 0x00]), None); // truncated item
        assert_eq!(top_level_usage(&[0xfe]), None); // truncated long item
        assert_eq!(top_level_usage(&[0xfe, 0xff, 0x00]), None); // long item past the end
        assert_eq!(top_level_usage(&[0xa1, 0x01]), None); // collection without usage
    }
}
