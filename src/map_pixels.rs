//! Memory-efficient authoritative state pixel lookup.
//! HOI4 state IDs normally fit into u16. Preserve larger modded IDs by
//! transparently switching to u32 rather than truncating them.
#[derive(Debug)]
pub enum StatePixelIds {
    Compact(Vec<u16>),
    Wide(Vec<u32>),
}

impl StatePixelIds {
    pub fn new(pixel_count: usize, max_state_id: u32) -> Self {
        if max_state_id <= u16::MAX as u32 {
            Self::Compact(vec![0; pixel_count])
        } else {
            Self::Wide(vec![0; pixel_count])
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Compact(data) => data.len(),
            Self::Wide(data) => data.len(),
        }
    }

    pub fn get(&self, index: usize) -> Option<u32> {
        match self {
            Self::Compact(data) => data.get(index).map(|id| u32::from(*id)),
            Self::Wide(data) => data.get(index).copied(),
        }
    }

    pub fn set(&mut self, index: usize, state_id: u32) {
        match self {
            Self::Compact(data) => data[index] = u16::try_from(state_id)
                .expect("Compact state IDs require values <= 65535"),
            Self::Wide(data) => data[index] = state_id,
        }
    }

    pub fn bytes_used(&self) -> usize {
        match self {
            Self::Compact(data) => data.capacity() * std::mem::size_of::<u16>(),
            Self::Wide(data) => data.capacity() * std::mem::size_of::<u32>(),
        }
    }

    #[cfg(test)]
    pub fn from_slice(ids: &[u32]) -> Self {
        let mut result = Self::new(ids.len(), ids.iter().copied().max().unwrap_or(0));
        for (index, &id) in ids.iter().enumerate() { result.set(index, id); }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_states_use_half_the_memory_without_losing_ids() {
        let ids = StatePixelIds::from_slice(&[0, 1, 1024, 65535]);
        assert!(matches!(ids, StatePixelIds::Compact(_)));
        assert_eq!(ids.bytes_used(), 8);
        assert_eq!(ids.len(), 4);
        assert_eq!(ids.get(3), Some(65535));
        assert_eq!(ids.get(4), None);
    }

    #[test]
    fn large_modded_ids_use_lossless_wide_fallback() {
        let ids = StatePixelIds::from_slice(&[0, 55, 65536, 500000]);
        assert!(matches!(ids, StatePixelIds::Wide(_)));
        assert_eq!(ids.bytes_used(), 16);
        assert_eq!(ids.get(3), Some(500000));
    }
}
