//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/UpdatableIntToIntMapTest.java`
//! and its subclasses `ListIntToIntMapTest.java`, `PermanentListIntToIntMapTest.java` and
//! `TreeIntToIntMapTest.java`: [`updatable_int_to_int_map_tests!`] expands the abstract test class
//! once per map.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ig_log::utils::flags::Flags;
use ig_log::utils::int_to_int_map::{IntToIntMap, UpdatableIntToIntMap};
use ig_log::utils::list_int_to_int_map::ListIntToIntMap;
use ig_log::utils::permanent_list_int_to_int_map::PermanentListIntToIntMap;
use ig_log::utils::predicate_flags::PredicateFlags;
use ig_log::utils::tree_int_to_int_map::TreeIntToIntMap;

type Visible = Rc<RefCell<HashSet<i32>>>;

/// `Integer.decode` of `|`-separated numbers.
fn parse_set(visibility: &str) -> HashSet<i32> {
    if visibility.is_empty() {
        return HashSet::new();
    }
    visibility
        .split('|')
        .map(|number| number.parse().unwrap())
        .collect()
}

struct Tester {
    visible_nodes: Visible,
    updatable_int_to_int_map: Box<dyn UpdatableIntToIntMap>,
}

impl Tester {
    fn change_visibility(&mut self, update_from: i32, update_to: i32, new_visibility: &str) {
        *self.visible_nodes.borrow_mut() = parse_set(new_visibility);

        self.updatable_int_to_int_map.update(update_from, update_to);

        assert_eq!(new_visibility, self.map_to_string());
    }

    fn map_to_string(&self) -> String {
        (0..self.updatable_int_to_int_map.short_size())
            .map(|short_index| {
                self.updatable_int_to_int_map
                    .get_long_index(short_index)
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn reverse_map_to_string(&self) -> String {
        (0..self.updatable_int_to_int_map.long_size())
            .map(|long_index| {
                self.updatable_int_to_int_map
                    .get_short_index(long_index)
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn test_long_to_short(&self, expected: &str) {
        assert_eq!(expected, self.reverse_map_to_string());
    }
}

fn get_test(
    create_updatable_int_to_int_map: fn(Visible, i32) -> Box<dyn UpdatableIntToIntMap>,
    long_size: i32,
    init_visibility: &str,
) -> Tester {
    let visible_nodes = Rc::new(RefCell::new(parse_set(init_visibility)));
    let updatable_int_to_int_map =
        create_updatable_int_to_int_map(visible_nodes.clone(), long_size);
    let tester = Tester {
        visible_nodes,
        updatable_int_to_int_map,
    };

    assert_eq!(init_visibility, tester.map_to_string());
    tester
}

fn is_visible(visible_nodes: Visible) -> impl Fn(i32) -> bool + Clone {
    move |integer| visible_nodes.borrow().contains(&integer)
}

/// `UpdatableIntToIntMapTest`'s test methods for one `createUpdatableIntToIntMap`.
macro_rules! updatable_int_to_int_map_tests {
    ($create:expr) => {
        use super::*;

        fn get_test(long_size: i32, init_visibility: &str) -> Tester {
            super::get_test($create, long_size, init_visibility)
        }

        #[test]
        fn simple_test() {
            let mut tester = get_test(6, "0|1|2|3|4|5");

            tester.change_visibility(1, 3, "0|2|4|5");
            tester.change_visibility(2, 2, "0|4|5");
            tester.change_visibility(1, 1, "0|1|4|5");
        }

        #[test]
        fn test_one_node() {
            let mut tester = get_test(1, "0");

            tester.change_visibility(0, 0, "");
            tester.change_visibility(0, 0, "");
            tester.change_visibility(0, 0, "0");
            tester.change_visibility(0, 0, "0");
            tester.change_visibility(0, 0, "");
        }

        #[test]
        fn test_two_nodes() {
            let mut tester = get_test(2, "0");

            tester.change_visibility(1, 1, "0|1");
            tester.change_visibility(0, 1, "");
            tester.change_visibility(0, 0, "0");
        }

        #[test]
        fn test4_nodes() {
            let mut tester = get_test(4, "2|3");

            tester.change_visibility(1, 3, "1");
            tester.change_visibility(0, 2, "0|1|2");
            tester.change_visibility(0, 0, "1|2");
        }

        #[test]
        fn test5_nodes() {
            let mut tester = get_test(5, "0|1|2|3|4");

            tester.change_visibility(4, 4, "0|1|2|3");
            tester.change_visibility(3, 4, "0|1|2|4");
        }

        #[test]
        fn test_reverse_map() {
            let mut tester = get_test(7, "0|1|2|3|4|5|6");

            tester.test_long_to_short("0|1|2|3|4|5|6");

            tester.change_visibility(2, 4, "0|1|5|6");

            tester.test_long_to_short("0|1|1|1|1|2|3");
        }

        #[test]
        fn test_reverse_map2() {
            let mut tester = get_test(8, "0|4|7");

            tester.test_long_to_short("0|0|0|0|1|1|1|2");

            tester.change_visibility(0, 0, "4|7");

            tester.test_long_to_short("0|0|0|0|0|0|0|1");
        }

        #[test]
        fn test_reverse_with_min_nodes() {
            let mut tester = get_test(1, "");

            tester.test_long_to_short("0");

            tester.change_visibility(0, 0, "0");

            tester.test_long_to_short("0");
        }

        #[test]
        fn test_reverse_with_min_nodes2() {
            let mut tester = get_test(2, "");

            tester.test_long_to_short("0|0");

            tester.change_visibility(1, 1, "1");

            tester.test_long_to_short("0|0");

            tester.change_visibility(0, 0, "0|1");

            tester.test_long_to_short("0|1");
        }

        #[test]
        fn empty_test() {
            let tester = get_test(0, "");
            tester.test_long_to_short("");
        }

        #[test]
        fn block_size_test() {
            get_test(0, "").test_long_to_short("");
            get_test(1, "0").test_long_to_short("0");
            get_test(2, "0|1").test_long_to_short("0|1");
            get_test(3, "0|1|2").test_long_to_short("0|1|2");
            get_test(4, "0|1|2|3").test_long_to_short("0|1|2|3");
            get_test(5, "0|1|2|3|4").test_long_to_short("0|1|2|3|4");
            get_test(6, "0|1|2|3|4|5").test_long_to_short("0|1|2|3|4|5");
            get_test(7, "0|1|2|3|4|5|6").test_long_to_short("0|1|2|3|4|5|6");
        }
    };
}

/// `ListIntToIntMapTest`
mod list_int_to_int_map_test {
    updatable_int_to_int_map_tests!(|this_is_visible, long_size| {
        Box::new(ListIntToIntMap::with_block_size(
            PredicateFlags::new(is_visible(this_is_visible), long_size),
            3,
        ))
    });
}

/// `TreeIntToIntMapTest`
mod tree_int_to_int_map_test {
    updatable_int_to_int_map_tests!(|this_is_visible, long_size| {
        Box::new(TreeIntToIntMap::new_instance(
            is_visible(this_is_visible),
            long_size,
        ))
    });
}

/// `PermanentListIntToIntMapTest`
mod permanent_list_int_to_int_map_test {
    updatable_int_to_int_map_tests!(|this_is_visible, long_size| {
        Box::new(UpdatableIntToIntMapWrapper::new(TestFlags {
            long_size,
            this_is_visible: is_visible(this_is_visible),
        }))
    });
}

/// The anonymous `Flags` of `PermanentListIntToIntMapTest.createUpdatableIntToIntMap`.
#[derive(Clone)]
struct TestFlags<P> {
    long_size: i32,
    this_is_visible: P,
}

impl<P: Fn(i32) -> bool> Flags for TestFlags<P> {
    fn size(&self) -> i32 {
        self.long_size
    }

    fn get(&self, index: i32) -> bool {
        (self.this_is_visible)(index)
    }

    fn set(&mut self, _index: i32, _value: bool) {
        unimplemented!()
    }

    fn set_all(&mut self, _value: bool) {
        unimplemented!()
    }
}

/// `PermanentListIntToIntMapTest.UpdatableIntToIntMapWrapper`: rebuilds the map on every update.
struct UpdatableIntToIntMapWrapper<P> {
    flags: TestFlags<P>,
    int_to_int_map: PermanentListIntToIntMap<TestFlags<P>>,
}

impl<P: Fn(i32) -> bool + Clone> UpdatableIntToIntMapWrapper<P> {
    fn new(flags: TestFlags<P>) -> Self {
        let int_to_int_map = Self::create_int_to_int_map(&flags);
        UpdatableIntToIntMapWrapper {
            flags,
            int_to_int_map,
        }
    }

    fn create_int_to_int_map(flags: &TestFlags<P>) -> PermanentListIntToIntMap<TestFlags<P>> {
        let mut short_size = 0;
        for i in 0..flags.size() {
            if flags.get(i) {
                short_size += 1;
            }
        }
        PermanentListIntToIntMap::with_block_size(flags.clone(), short_size, 2)
    }
}

impl<P: Fn(i32) -> bool + Clone> UpdatableIntToIntMap for UpdatableIntToIntMapWrapper<P> {
    fn update(&mut self, _start_long_index: i32, _end_long_index: i32) {
        self.int_to_int_map = Self::create_int_to_int_map(&self.flags);
    }
}

impl<P: Fn(i32) -> bool + Clone> IntToIntMap for UpdatableIntToIntMapWrapper<P> {
    fn short_size(&self) -> i32 {
        self.int_to_int_map.short_size()
    }

    fn long_size(&self) -> i32 {
        self.int_to_int_map.long_size()
    }

    fn get_long_index(&self, short_index: i32) -> i32 {
        self.int_to_int_map.get_long_index(short_index)
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        self.int_to_int_map.get_short_index(long_index)
    }
}
