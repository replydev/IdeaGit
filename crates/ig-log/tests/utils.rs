//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/BitSetFlagsTest.java`
//! and `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/IntIntMultiMapTest.kt`.

mod bit_set_flags_test {
    use ig_log::utils::flags::{BitSetFlags, Flags};

    fn bit_to_char(bit: bool) -> char {
        if bit { '1' } else { '0' }
    }

    fn to_str(flags: &dyn Flags) -> String {
        (0..flags.size())
            .map(|i| bit_to_char(flags.get(i)))
            .collect()
    }

    #[test]
    fn init_test() {
        let flags = BitSetFlags::new(5);
        assert_eq!("00000", to_str(&flags));
        assert_eq!(5, flags.size());
    }

    #[test]
    fn set_test() {
        let mut flags = BitSetFlags::new(6);
        assert_eq!("000000", to_str(&flags));

        flags.set(0, true);
        assert_eq!("100000", to_str(&flags));

        flags.set(0, false);
        assert_eq!("000000", to_str(&flags));

        flags.set(4, true);
        assert_eq!("000010", to_str(&flags));

        flags.set(2, true);
        assert_eq!("001010", to_str(&flags));
    }

    #[test]
    fn set_all_test() {
        let mut flags = BitSetFlags::new(6);
        assert_eq!("000000", to_str(&flags));

        flags.set_all(false);
        assert_eq!("000000", to_str(&flags));

        flags.set_all(true);
        assert_eq!("111111", to_str(&flags));
    }

    #[test]
    fn size1_test() {
        let mut flags = BitSetFlags::new(1);
        assert_eq!("0", to_str(&flags));
        assert_eq!(1, flags.size());

        flags.set(0, true);
        assert_eq!("1", to_str(&flags));

        flags.set(0, false);
        assert_eq!("0", to_str(&flags));

        flags.set_all(true);
        assert_eq!("1", to_str(&flags));
    }

    #[test]
    fn empty_flags_test() {
        let mut flags = BitSetFlags::new(0);
        assert_eq!(0, flags.size());
        flags.set_all(true);
        assert_eq!(0, flags.size());
    }
}

mod int_int_multi_map_test {
    use std::collections::BTreeMap;

    use ig_log::utils::int_int_multi_map::IntIntMultiMap;

    /// Compares `IntIntMultiMap` with a reference multimap (upstream: IntelliJ's `MultiMap`).
    #[derive(Default)]
    struct MapTester {
        correct_map: BTreeMap<i32, Vec<i32>>,
        tested_map: IntIntMultiMap,
    }

    fn sorted(values: &[i32]) -> Vec<i32> {
        let mut values = values.to_vec();
        values.sort();
        values
    }

    fn join_values(values: &[i32]) -> String {
        values
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }

    impl MapTester {
        fn put_value(&mut self, key: i32, value: i32) {
            self.correct_map.entry(key).or_default().push(value);
            self.tested_map.put_value(key, value);
            self.compare_maps();
        }

        fn remove(&mut self, key: i32, value: i32) {
            if let Some(values) = self.correct_map.get_mut(&key) {
                if let Some(i) = values.iter().position(|&v| v == value) {
                    values.remove(i);
                }
                if values.is_empty() {
                    self.correct_map.remove(&key);
                }
            }
            self.tested_map.remove(key, value);
            self.compare_maps();
        }

        fn compare_maps(&self) {
            let keys = self.compare_keys();
            let lines = |f: &dyn Fn(i32) -> String| {
                keys.iter().map(|&k| f(k)).collect::<Vec<_>>().join("\n")
            };
            let correct_str =
                lines(&|k| format!("{k}:{}", join_values(&sorted(&self.correct_map[&k]))));
            let tested_str =
                lines(&|k| format!("{k}:{}", join_values(&sorted(self.tested_map.get(k)))));
            let tested_array_str = lines(&|k| {
                format!(
                    "{k}:{}",
                    join_values(&sorted(self.tested_map.get_as_array(k)))
                )
            });
            assert_eq!(
                tested_array_str, tested_str,
                "get and getasArray have nonequal results"
            );
            assert_eq!(correct_str, tested_str, "maps non equals");
        }

        fn compare_keys(&self) -> Vec<i32> {
            let correct_keys: Vec<i32> = self.correct_map.keys().copied().collect();
            let tested_keys = sorted(&self.tested_map.keys());
            assert_eq!(join_values(&correct_keys), join_values(&tested_keys));
            correct_keys
        }
    }

    fn run_test(test: impl FnOnce(&mut MapTester)) {
        test(&mut MapTester::default());
    }

    #[test]
    fn simple() {
        run_test(|t| {
            t.put_value(2, 239);
            t.put_value(2, 17);
            t.put_value(2, 18);
            t.put_value(8, 18);
            t.put_value(8, 22);

            t.remove(2, 2);
            t.put_value(2, 2);
            t.remove(2, 17);
            t.remove(8, 18);
        });
    }

    #[test]
    fn remove_one_value() {
        run_test(|t| {
            t.put_value(1, 2);
            t.remove(1, 2);
        });
    }

    #[test]
    fn remove_two_value() {
        run_test(|t| {
            t.put_value(1, 2);
            t.put_value(1, 3);
            t.remove(1, 2);
            t.remove(1, 3);
        });
    }

    #[test]
    fn add_different_keys() {
        run_test(|t| {
            t.put_value(1, 1);
            t.put_value(2, 10);
            t.put_value(1, 3);
            t.put_value(2, 30);
            t.remove(1, 3);
        });
    }

    #[test]
    fn remove_for_non_existed_key() {
        run_test(|t| {
            t.remove(1, 3);

            t.put_value(2, 5);
            t.remove(3, 3);
        });
    }
}
