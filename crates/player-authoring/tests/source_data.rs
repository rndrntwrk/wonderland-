use wonderland_player_authoring::*;
#[test]
fn original_catalog_source_preserves_identity_price_category_and_disable_level() {
    let rows = parse_catalog_xml(include_str!(
        "../../../TSOClient/FSO.Content.TSO/Content/Objects/catalog_downloads.xml"
    ))
    .unwrap();
    let fungi = rows.iter().find(|row| row.guid == 0x0FE699E4).unwrap();
    assert_eq!(fungi.price, 15);
    assert_eq!(fungi.category, 3);
    assert_eq!(fungi.name, "Fly Agaric Fungi");
    let custom=parse_catalog_xml("<Catalog><P g='FFFFFFFF' s='27' p='4294967295' r='3' n='Source &amp; Item'/><P g='12' s='-1' p='1' n='Hidden'/></Catalog>").unwrap();
    assert_eq!(custom.len(), 1);
    assert_eq!(custom[0].guid, u32::MAX);
    assert_eq!(custom[0].price, u32::MAX);
    assert_eq!(custom[0].category, 27);
    assert_eq!(custom[0].disable_level, 3);
    assert_eq!(custom[0].name, "Source & Item");
}
#[test]
fn owned_outfit_uses_original_big_endian_stock_and_uint16_enum_fields() {
    let bytes = [
        0, 0, 0, 1, 0x12, 0x34, 0x56, 0x78, 0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0, 0,
        3, 232, 0, 0, 1, 244, 0, 1, 0, 0, 0, 42, 5, 0, 1,
    ];
    let rows = decode_outfit_stock(&bytes).unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.outfit_id, 0x12345678);
    assert_eq!(row.asset_id, 0xffeeddccbbaa9988);
    assert_eq!(row.sale_price, 1000);
    assert_eq!(row.purchase_price, 500);
    assert_eq!(row.owner_type, 1);
    assert_eq!(row.owner_id, 42);
    assert_eq!(row.category, 5);
    assert_eq!(row.source, 1);
    let json = serde_json::to_string(row).unwrap();
    assert!(
        json.contains("\"18441921395520346504\""),
        "asset keys must serialize losslessly: {json}"
    );
    for n in 0..bytes.len() {
        assert!(decode_outfit_stock(&bytes[..n]).is_err());
    }
}
#[test]
fn source_discount_and_donation_use_integer_rounding_without_inventing_a_quote() {
    assert_eq!(source_purchase_price(1001, 15, 1).unwrap(), 850);
    assert_eq!(source_purchase_price(1001, 15, 2).unwrap(), 284);
    assert!(source_purchase_price(100, 101, 1).is_err());
    assert!(source_purchase_price(100, 0, 0).is_err());
}
