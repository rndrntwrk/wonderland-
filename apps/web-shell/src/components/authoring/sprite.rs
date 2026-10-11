use crate::authoring_geometry::{LotArtTransform, sprite_layout};
use leptos::prelude::*;
use wonderland_contracts::authoring::{CatalogItem, GridCell, GridPose, LotBounds};
#[component]
pub fn Furniture(
    item: CatalogItem,
    pose: GridPose,
    #[prop(default = LotBounds { origin: GridCell{x:0,y:0},width:8,depth:6 })] bounds: LotBounds,
    #[prop(default = false)] ghost: bool,
) -> impl IntoView {
    let center = LotArtTransform::new(bounds).footprint_center(item.footprint, pose);
    let sprite = sprite_layout(item.id.as_ref(), pose.direction);
    view! {<img class="furniture-art" class:ghost=ghost src=sprite.path alt="" draggable="false" style=format!("left:{}px;top:{}px;width:{}px;height:{}px;transform:translate({}px,{}px) scaleX({});z-index:{}",center.x,center.y,sprite.width,sprite.height,-sprite.anchor.x,-sprite.anchor.y,if sprite.mirror{-1}else{1},if item.id.as_ref()=="woven-rug"{1}else{center.y as i32+10})/>}
}
