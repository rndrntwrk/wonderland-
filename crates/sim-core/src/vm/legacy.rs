//! Logical TS1 neighborhood projections. The content/persistence adapter owns import and saving.
use super::VmFault;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_TS1_INVENTORY_ITEMS: usize = 65_536;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ts1InventoryItem {
    pub token_type: i32,
    pub guid: u32,
    pub count: u16,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ts1InventoryBook {
    pub inventories: BTreeMap<i16, Vec<Ts1InventoryItem>>,
}
impl Ts1InventoryBook {
    pub fn read(&self, neighbor: i16) -> Option<Vec<Ts1InventoryItem>> {
        self.inventories.get(&neighbor).cloned()
    }
    pub fn write(&mut self, neighbor: i16, items: Vec<Ts1InventoryItem>) -> Result<(), VmFault> {
        let others = self
            .inventories
            .iter()
            .filter(|(key, _)| **key != neighbor)
            .fold(0usize, |count, (_, items)| {
                count.saturating_add(items.len())
            });
        if items.len() > MAX_TS1_INVENTORY_ITEMS
            || others.saturating_add(items.len()) > MAX_TS1_INVENTORY_ITEMS
        {
            return Err(VmFault::InvalidContent("TS1 inventory item limit".into()));
        }
        self.inventories.insert(neighbor, items);
        Ok(())
    }
    pub fn validate(&self) -> Result<(), VmFault> {
        let count = self
            .inventories
            .values()
            .fold(0usize, |count, items| count.saturating_add(items.len()));
        if self.inventories.len() > 65_536 || count > MAX_TS1_INVENTORY_ITEMS {
            Err(VmFault::InvalidContent("TS1 inventory bounds".into()))
        } else {
            Ok(())
        }
    }
}
