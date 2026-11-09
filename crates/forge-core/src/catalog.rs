//! Individual product targets from the supplied catalog. Family routing shares
//! infrastructure; it does not imply equivalence between separate applications.
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkArea {
    Draft,
    Model,
    Build,
    Terrain,
    Make,
    Motion,
    Connect,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductKind {
    Application,
    Extension,
    Collection,
    Service,
    Sdk,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Active,
    Discontinued,
    RenewalOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationStatus {
    Prototype,
    Planned,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub area: WorkArea,
    pub kind: ProductKind,
    pub lifecycle: Lifecycle,
    pub status: ImplementationStatus,
}

static CATALOG: LazyLock<Vec<Product>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/products.json"))
        .expect("embedded product catalog must be valid; covered by catalog tests")
});

pub fn products() -> &'static [Product] {
    &CATALOG
}
pub fn product(id: &str) -> Option<&'static Product> {
    products().iter().find(|p| p.id == id)
}
pub fn search(query: &str) -> Vec<&'static Product> {
    let query = query.trim().to_lowercase();
    products()
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&query) || p.id.contains(&query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    #[test]
    fn every_supplied_target_has_unique_identity() {
        assert_eq!(products().len(), 140);
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for p in products() {
            assert!(!p.id.is_empty() && !p.name.is_empty());
            assert!(
                p.id.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            );
            assert!(ids.insert(&p.id), "duplicate id {}", p.id);
            assert!(names.insert(&p.name), "duplicate name {}", p.name);
        }
    }
    #[test]
    fn distinct_major_products_are_retained() {
        for id in [
            "autocad",
            "autocad-lt",
            "inventor",
            "revit",
            "fusion",
            "maya",
            "3ds-max",
            "civil-3d",
            "navisworks-manage",
            "moldflow-insight",
            "vault-professional",
        ] {
            assert!(product(id).is_some(), "missing {id}");
        }
        assert_ne!(product("autocad").unwrap(), product("autocad-lt").unwrap());
        assert_eq!(search("AutoCAD").len(), 14);
    }
    #[test]
    fn lifecycle_information_is_retained() {
        assert_eq!(
            product("autocad-design-suite").unwrap().lifecycle,
            Lifecycle::Discontinued
        );
        assert_eq!(
            product("constructware").unwrap().lifecycle,
            Lifecycle::RenewalOnly
        );
    }
}
