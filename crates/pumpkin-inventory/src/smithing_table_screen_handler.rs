use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::crafting::recipe_provider::RecipeProvider;
use crate::player::player_inventory::PlayerInventory;
use crate::screen_handler::{InventoryPlayer, ScreenHandler, ScreenHandlerBehaviour};
use crate::slot::Slot;

use crate::inventory::Inventory;
use crate::inventory::SimpleInventory;
use pumpkin_data::data_component_impl::{ProvidesTrimMaterialImpl, TrimImpl};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::recipes::{get_smithing_transform_recipe, get_smithing_trim_recipe};
use pumpkin_data::screen::WindowType;
use pumpkin_data::statistic::StatisticCategory;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_protocol::codec::recipe::DynamicRecipe;
use pumpkin_protocol::java::server::play::SlotActionType;
use tracing::warn;

pub struct SmithingTableScreenHandler {
    behaviour: ScreenHandlerBehaviour,
    pub input_inventory: Arc<SimpleInventory>,
    pub output_inventory: Arc<SimpleInventory>,
    pub recipe_provider: Option<Arc<dyn RecipeProvider>>,
}

impl SmithingTableScreenHandler {
    pub fn new(
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        recipe_provider: Option<Arc<dyn RecipeProvider>>,
    ) -> Self {
        let behaviour = ScreenHandlerBehaviour::new(sync_id, Some(WindowType::Smithing));
        let input_inventory = Arc::new(SimpleInventory::new(3));
        let output_inventory = Arc::new(SimpleInventory::new(1));

        let mut handler = Self {
            behaviour,
            input_inventory,
            output_inventory,
            recipe_provider,
        };

        handler.add_slot(Arc::new(SmithingInputSlot::new(
            handler.input_inventory.clone(),
            0,
            0, // Template
        )));
        handler.add_slot(Arc::new(SmithingInputSlot::new(
            handler.input_inventory.clone(),
            1,
            1, // Base
        )));
        handler.add_slot(Arc::new(SmithingInputSlot::new(
            handler.input_inventory.clone(),
            2,
            2, // Addition
        )));
        handler.add_slot(Arc::new(SmithingOutputSlot::new(
            handler.output_inventory.clone(),
            handler.input_inventory.clone(),
            0,
        )));

        let player_inventory: Arc<dyn Inventory> = player_inventory.clone();
        handler.add_player_slots(&player_inventory);

        handler
    }

    fn find_transform_result(
        &self,
        template: &ItemStack,
        base: &ItemStack,
        addition: &ItemStack,
    ) -> Option<ItemStack> {
        warn!(
            "[SMITHING DEBUG] find_transform_result: template={}, base={}, addition={}",
            template.get_item().registry_key,
            base.get_item().registry_key,
            addition.get_item().registry_key
        );

        if let Some(recipe) = get_smithing_transform_recipe(template.item, base.item, addition.item)
        {
            warn!("[SMITHING DEBUG] Found static transform recipe: {:?}", recipe.result);
            let res_key = recipe
                .result
                .id
                .strip_prefix("minecraft:")
                .unwrap_or(recipe.result.id);
            if let Some(target_item) = Item::from_registry_key(res_key) {
                let mut result = base.clone();
                result.item = target_item;
                result.item_count = recipe.result.count;
                return Some(result);
            }
        }

        if let Some(provider) = &self.recipe_provider {
            for dynamic in provider.get_dynamic_recipes() {
                if let DynamicRecipe::SmithingTransform(recipe) = dynamic {
                    warn!("[SMITHING DEBUG] Checking dynamic transform recipe: {:?}", recipe.result);
                    if recipe.template.match_item(template.item)
                        && recipe.base.match_item(base.item)
                        && recipe.addition.match_item(addition.item)
                    {
                        warn!("[SMITHING DEBUG] Dynamic transform recipe MATCHED");
                        let res_key = recipe
                            .result
                            .item_id
                            .strip_prefix("minecraft:")
                            .unwrap_or(&recipe.result.item_id);
                        if let Some(target_item) = Item::from_registry_key(res_key) {
                            let mut result = base.clone();
                            result.item = target_item;
                            result.item_count = recipe.result.count;
                            return Some(result);
                        }
                    }
                }
            }
        }
        warn!("[SMITHING DEBUG] No transform result found");
        None
    }

    fn find_trim_pattern(
        &self,
        template: &ItemStack,
        base: &ItemStack,
        addition: &ItemStack,
    ) -> Option<String> {
        warn!(
            "[SMITHING DEBUG] find_trim_pattern: template={}, base={}, addition={}",
            template.get_item().registry_key,
            base.get_item().registry_key,
            addition.get_item().registry_key
        );

        if let Some(recipe) = get_smithing_trim_recipe(template.item, base.item, addition.item) {
            warn!("[SMITHING DEBUG] Found static trim recipe: {}", recipe.pattern);
            return Some(recipe.pattern.to_string());
        }
        let provider = self.recipe_provider.as_ref()?;
        let result = provider.get_dynamic_recipes().iter().find_map(|d| {
            if let DynamicRecipe::SmithingTrim(recipe) = d
                && recipe.template.match_item(template.item)
                && recipe.base.match_item(base.item)
                && recipe.addition.match_item(addition.item)
            {
                let pattern = template
                    .item
                    .registry_key
                    .strip_suffix("_armor_trim_smithing_template")
                    .unwrap_or(template.item.registry_key);
                warn!("[SMITHING DEBUG] Dynamic trim recipe MATCHED: {}", pattern);
                Some(pattern.to_string())
            } else {
                None
            }
        });
        if result.is_none() {
            warn!("[SMITHING DEBUG] No trim pattern found");
        }
        result
    }

    pub fn update_output(&self) {
        let template_stack = self.input_inventory.get_stack(0);
        let base_stack = self.input_inventory.get_stack(1);
        let addition_stack = self.input_inventory.get_stack(2);

        warn!(
            "[SMITHING DEBUG] update_output: template={}, base={}, addition={}",
            template_stack.get_item().registry_key,
            base_stack.get_item().registry_key,
            addition_stack.get_item().registry_key
        );

        if template_stack.is_empty() || base_stack.is_empty() || addition_stack.is_empty() {
            warn!("[SMITHING DEBUG] One or more slots are empty, clearing output");
            self.output_inventory.set_stack(0, ItemStack::EMPTY.clone());
            return;
        }

        if let Some(result) =
            self.find_transform_result(&template_stack, &base_stack, &addition_stack)
        {
            warn!("[SMITHING DEBUG] Transform result: {}", result.get_item().registry_key);
            self.output_inventory.set_stack(0, result);
            return;
        }

        if let Some(pattern) = self.find_trim_pattern(&template_stack, &base_stack, &addition_stack)
            && let Some(provides) = addition_stack.get_data_component::<ProvidesTrimMaterialImpl>()
        {
            let material = provides.material.as_ref();
            warn!("[SMITHING DEBUG] Trim pattern: {}, material: {}", pattern, material);
            
            if let Some(trim) = base_stack.get_data_component::<TrimImpl>() {
                let curr_mat = match &trim.material {
                    NbtTag::String(s) => s.as_ref(),
                    _ => "",
                };
                let curr_pat = match &trim.pattern {
                    NbtTag::String(s) => s.as_ref(),
                    _ => "",
                };
                if curr_mat == material && curr_pat == pattern {
                    warn!("[SMITHING DEBUG] Same trim already applied, clearing output");
                    self.output_inventory.set_stack(0, ItemStack::EMPTY.clone());
                    return;
                }
            }

            let mut result = base_stack;
            result.item_count = 1;
            result.set_data_component(TrimImpl {
                material: NbtTag::String(material.to_string().into()),
                pattern: NbtTag::String(pattern.clone().into()),
            });

            warn!("[SMITHING DEBUG] Setting output to trim: material={}, pattern={}", material, pattern);
            self.output_inventory.set_stack(0, result);
            return;
        }

        warn!("[SMITHING DEBUG] No valid recipe, clearing output");
        self.output_inventory.set_stack(0, ItemStack::EMPTY.clone());
    }
}

impl ScreenHandler for SmithingTableScreenHandler {
    fn get_behaviour(&self) -> &ScreenHandlerBehaviour {
        &self.behaviour
    }

    fn get_behaviour_mut(&mut self) -> &mut ScreenHandlerBehaviour {
        &mut self.behaviour
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn on_slot_click(
        &mut self,
        slot_index: i32,
        button: i32,
        action_type: SlotActionType,
        player: &dyn InventoryPlayer,
    ) {
        warn!("[SMITHING DEBUG] on_slot_click: slot={}, button={}, action={:?}", slot_index, button, action_type);
        self.internal_on_slot_click(slot_index, button, action_type, player);
        if (0..=3).contains(&slot_index) {
            self.update_output();
        }
    }

    fn quick_move(&mut self, player: &dyn InventoryPlayer, slot_index: i32) -> ItemStack {
        warn!("[SMITHING DEBUG] quick_move: slot={}", slot_index);
        let mut stack = ItemStack::EMPTY.clone();
        let slot = self.get_behaviour().slots.get(slot_index as usize).cloned();

        if let Some(slot) = slot {
            let mut slot_stack = slot.get_cloned_stack();
            if !slot_stack.is_empty() {
                stack = slot_stack.clone();
                match slot_index.cmp(&3) {
                    std::cmp::Ordering::Equal => {
                        // From output slot to player inventory
                        if !self.insert_item(&mut slot_stack, 4, 40, true) {
                            return ItemStack::EMPTY.clone();
                        }
                        slot.on_quick_move_crafted(slot_stack.clone(), stack.clone());
                        let mut taken_stack = stack.clone();
                        taken_stack.set_count(stack.item_count - slot_stack.item_count);
                        slot.on_take_item(player, &taken_stack);
                    }
                    std::cmp::Ordering::Less => {
                        // From input slots to player inventory
                        if !self.insert_item(&mut slot_stack, 4, 40, true) {
                            return ItemStack::EMPTY.clone();
                        }
                    }
                    std::cmp::Ordering::Greater => {
                        // From player inventory into input slots (0..3)
                        if !self.insert_item(&mut slot_stack, 0, 3, false) {
                            return ItemStack::EMPTY.clone();
                        }
                    }
                }
                self.update_output();

                if slot_stack.is_empty() {
                    slot.set_stack(ItemStack::EMPTY.clone());
                } else {
                    slot.set_stack(slot_stack);
                }
            }
        }
        stack
    }

    fn on_closed(&mut self, player: &dyn InventoryPlayer) {
        warn!("[SMITHING DEBUG] on_closed");
        self.default_on_closed(player);
        self.drop_inventory(player, self.input_inventory.clone());
    }
}

pub struct SmithingInputSlot {
    pub inventory: Arc<dyn Inventory>,
    pub index: usize,
    pub slot_type: u8, // 0 = Template, 1 = Base, 2 = Addition
    pub id: AtomicU8,
}

impl SmithingInputSlot {
    pub fn new(inventory: Arc<dyn Inventory>, index: usize, slot_type: u8) -> Self {
        Self {
            inventory,
            index,
            slot_type,
            id: AtomicU8::new(0),
        }
    }

    fn is_valid_for_slot(&self, stack: &ItemStack) -> bool {
        if stack.is_empty() {
            return true;
        }

        let item = stack.item;

        let valid = match self.slot_type {
            0 => {
                item.registry_key.contains("smithing_template") || item.registry_key.contains("_armor_trim")
            }
            1 => {
                item.registry_key.contains("helmet") || item.registry_key.contains("chestplate")
                || item.registry_key.contains("leggings") || item.registry_key.contains("boots")
                || item.registry_key.contains("sword") || item.registry_key.contains("pickaxe")
                || item.registry_key.contains("axe") || item.registry_key.contains("shovel")
                || item.registry_key.contains("hoe")
            }
            2 => {
                item.registry_key.contains("ingot") || item.registry_key.contains("diamond")
                || item.registry_key.contains("emerald") || item.registry_key.contains("quartz")
                || item.registry_key.contains("netherite_scrap") || item.registry_key.contains("lapis")
                || item.registry_key.contains("redstone")
            }
            _ => false,
        };

        warn!(
            "[SMITHING DEBUG] is_valid_for_slot: item={}, slot_type={}, valid={}",
            item.registry_key, self.slot_type, valid
        );

        valid
    }
}

impl Slot for SmithingInputSlot {
    fn get_inventory(&self) -> Arc<dyn Inventory> {
        self.inventory.clone()
    }

    fn get_index(&self) -> usize {
        self.index
    }

    fn set_id(&self, id: usize) {
        self.id.store(id as u8, Ordering::Relaxed);
    }

    fn get_stack(&self) -> ItemStack {
        self.inventory.get_stack(self.index)
    }

    fn get_cloned_stack(&self) -> ItemStack {
        self.inventory.get_stack(self.index)
    }

    fn has_stack(&self) -> bool {
        !self.inventory.get_stack(self.index).is_empty()
    }

    fn set_stack(&self, stack: ItemStack) {
        warn!(
            "[SMITHING DEBUG] set_stack: slot={}, item={}, count={}",
            self.index, stack.get_item().registry_key, stack.item_count
        );
        self.inventory.set_stack(self.index, stack);
    }

    fn set_stack_prev(&self, _stack: ItemStack, _previous_stack: ItemStack) {
        // Do nothing
    }

    fn mark_dirty(&self) {
        self.inventory.mark_dirty();
    }

    fn can_insert(&self, stack: &ItemStack) -> bool {
        self.is_valid_for_slot(stack)
    }
}

pub struct SmithingOutputSlot {
    pub inventory: Arc<dyn Inventory>,
    pub input_inventory: Arc<dyn Inventory>,
    pub index: usize,
    pub id: AtomicU8,
}

impl SmithingOutputSlot {
    pub fn new(
        inventory: Arc<dyn Inventory>,
        input_inventory: Arc<dyn Inventory>,
        index: usize,
    ) -> Self {
        Self {
            inventory,
            input_inventory,
            index,
            id: AtomicU8::new(0),
        }
    }
}

impl Slot for SmithingOutputSlot {
    fn get_inventory(&self) -> Arc<dyn Inventory> {
        self.inventory.clone()
    }

    fn get_index(&self) -> usize {
        self.index
    }

    fn set_id(&self, id: usize) {
        self.id.store(id as u8, Ordering::Relaxed);
    }

    fn on_take_item(&self, player: &dyn InventoryPlayer, stack: &ItemStack) {
        warn!("[SMITHING DEBUG] on_take_item: item={}, count={}", stack.get_item().registry_key, stack.item_count);
        player.increment_stat(
            StatisticCategory::Crafted,
            stack.item.id as i32,
            stack.item_count as i32,
        );
        self.input_inventory.remove_stack_specific(0, 1);
        self.input_inventory.remove_stack_specific(1, 1);
        self.input_inventory.remove_stack_specific(2, 1);
        self.mark_dirty();
    }

    fn can_insert(&self, _stack: &ItemStack) -> bool {
        false
    }

    fn get_stack(&self) -> ItemStack {
        self.inventory.get_stack(self.index)
    }

    fn get_cloned_stack(&self) -> ItemStack {
        self.inventory.get_stack(self.index)
    }

    fn has_stack(&self) -> bool {
        !self.inventory.get_stack(self.index).is_empty()
    }

    fn set_stack(&self, stack: ItemStack) {
        warn!("[SMITHING DEBUG] output set_stack: item={}, count={}", stack.get_item().registry_key, stack.item_count);
        self.inventory.set_stack(self.index, stack);
    }

    fn set_stack_prev(&self, _stack: ItemStack, _previous_stack: ItemStack) {
        // Do nothing
    }

    fn mark_dirty(&self) {
        self.inventory.mark_dirty();
    }
}
