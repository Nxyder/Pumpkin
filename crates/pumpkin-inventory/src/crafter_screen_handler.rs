use std::any::Any;
use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::screen::WindowType;

use crate::inventory::Inventory;
use crate::player::player_inventory::PlayerInventory;
use crate::screen_handler::{InventoryPlayer, ScreenHandler, ScreenHandlerBehaviour};
use crate::slot::NormalSlot;

/// Een screen handler specifiek voor de Crafter.
/// Deze luistert naar de "slot toggle" actie van de client.
pub struct CrafterScreenHandler {
    pub inventory: Arc<dyn Inventory>,
    behaviour: ScreenHandlerBehaviour,
}

impl CrafterScreenHandler {
    pub fn new(
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        inventory: Arc<dyn Inventory>,
        player: &dyn InventoryPlayer,
    ) -> Self {
        let mut handler = Self {
            inventory,
            behaviour: ScreenHandlerBehaviour::new(sync_id, Some(WindowType::Crafter3x3)),
        };

        if !player.is_spectator() {
            handler.inventory.on_open();
        }

        // Voeg de 9 Crafter-slots toe
        for i in 0..9 {
            handler.add_slot(Arc::new(NormalSlot::new(handler.inventory.clone(), i)));
        }

        let player_inv: Arc<dyn Inventory> = player_inventory.clone();
        handler.add_player_slots(&player_inv);

        handler
    }

    /// Deze functie wordt aangeroepen wanneer de client op een slot klikt
    /// om het in/uit te schakelen. Dit is de missing link in Pumpkin.
    pub fn on_slot_toggle(&mut self, slot: usize, disabled: bool) {
        // We casten de inventaris terug naar de CrafterBlockEntity
        // om toegang te krijgen tot de set_slot_state methode.
        if let Some(crafter) = self.inventory.as_any().downcast_ref::<crate::block::entities::crafter::CrafterBlockEntity>() {
            // enabled = !disabled (disabled=true betekent enabled=false)
            crafter.set_slot_state(slot, !disabled);
        }
    }
}

impl ScreenHandler for CrafterScreenHandler {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn get_behaviour(&self) -> &ScreenHandlerBehaviour { &self.behaviour }
    fn get_behaviour_mut(&mut self) -> &mut ScreenHandlerBehaviour { &mut self.behaviour }

    fn on_closed(&mut self, player: &dyn InventoryPlayer) {
        self.default_on_closed(player);
        if !player.is_spectator() {
            self.inventory.on_close();
        }
    }

    fn quick_move(&mut self, _player: &dyn InventoryPlayer, slot_index: i32) -> ItemStack {
        // Zelfde logica als GenericContainerScreenHandler
        let mut stack_left = ItemStack::EMPTY.clone();
        let slot = self.get_behaviour().slots[slot_index as usize].clone();

        if slot.has_stack() {
            let mut slot_stack = slot.get_stack();
            stack_left = slot_stack.clone();

            if slot_index < 9 {
                if !self.insert_item(&mut slot_stack, 9, self.get_behaviour().slots.len() as i32, true) {
                    return ItemStack::EMPTY.clone();
                }
            } else if !self.insert_item(&mut slot_stack, 0, 9, false) {
                return ItemStack::EMPTY.clone();
            }

            if slot_stack.is_empty() {
                slot.set_stack(ItemStack::EMPTY.clone());
            } else {
                slot.set_stack(slot_stack);
            }
        }
        stack_left
    }
}
