//! Deterministic, sortable string IDs.

pub fn system_id(index: usize) -> String {
    format!("sys-{:04}", index)
}

pub fn world_id(system_index: usize, world_index: usize) -> String {
    format!("sys-{:04}-w{:02}", system_index, world_index)
}

pub fn route_id(a: &str, b: &str) -> String {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    format!("route-{}-{}", lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_ids_are_stable() {
        assert_eq!(system_id(1), "sys-0001");
        assert_eq!(system_id(42), "sys-0042");
        assert_eq!(system_id(9999), "sys-9999");
    }

    #[test]
    fn world_ids_are_stable() {
        assert_eq!(world_id(1, 1), "sys-0001-w01");
        assert_eq!(world_id(7, 12), "sys-0007-w12");
    }

    #[test]
    fn route_id_orders_system_ids() {
        let a = system_id(2);
        let b = system_id(7);
        assert_eq!(route_id(&a, &b), "route-sys-0002-sys-0007");
        assert_eq!(route_id(&b, &a), "route-sys-0002-sys-0007");
    }
}
