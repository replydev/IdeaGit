//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/LinearBekTest.kt`.

mod common;

use common::linear_bek::{assert_graph_equals, run_linear_bek};
use common::test_graph_builder::{TestLinearGraph, test_graph};

fn run_test(before_linear_bek: TestLinearGraph, after_linear_bek: TestLinearGraph) {
    assert_graph_equals(after_linear_bek, &run_linear_bek(before_linear_bek));
}

/*
0         0
| \       |
| 1       1
| 2       2
| 3       3
| |       :
4 |  ->   4
5 |       5
6 |       6
|/        |
7         7
*/
#[test]
fn simple_graph_test() {
    run_test(
        test_graph! {
            0(4, 1)
            1(2)
            2(3)
            3(7)
            4(5)
            5(6)
            6(7)
            7()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4.dot)
            4(5)
            5(6)
            6(7)
            7()
        },
    )
}

/*
0           0
|\          |
| 1         1
| |         :
2 |         2
|/          |
3    ->     3
|\          |
| 4         4
| |         :
5 |         5
|/          |
6           6
 */
#[test]
fn two_sections_graph_test() {
    run_test(
        test_graph! {
            0(2, 1)
            1(3)
            2(3)
            3(5, 4)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2(3)
            3(4)
            4(5.dot)
            5(6)
            6()
        },
    )
}

/*
0              0
|\             |
| 1            1
| |\           |
| | 2          2
| | |          :
| 3 |   ->     3
| |/           |
| 4            4
| |            :
5 |            5
|/             |
6              6
 */
#[test]
fn recursive_sections_test() {
    run_test(
        test_graph! {
            0(5, 1)
            1(3, 2)
            2(4)
            3(4)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2)
            2(3.dot)
            3(4)
            4(5.dot)
            5(6)
            6()
        },
    )
}

/*
0             0
|\            |
| 1           1
| |           :
2 |           2
|\|           |
| 3     ->    3
| |           :
4 |           4
|/            |
5             5
 */
#[test]
fn diagonal_test() {
    run_test(
        test_graph! {
            0(2, 1)
            1(3)
            2(4, 3)
            3(5)
            4(5)
            5()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2(3)
            3(4.dot)
            4(5)
            5()
        },
    )
}

/*
0             0
|\            |
| 1           1
|/|    ->     :
2 |           2
\|            :
3             3
 */
#[test]
fn different_diagonal_test() {
    run_test(
        test_graph! {
            0(2, 1)
            1(2, 3)
            2(3)
            3()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2(3.dot)
            3()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| 2 \            2
| |\|            :
| | 3            3
| \|             :
| 4              4
| \      ->      :
5  \             5
|\  \            |
6 \ |            6
|\| |            :
| 7 |            7
|/  |            :
8  /             8
|/               |
9                9
 */
#[test]
fn complicated_branches_test() {
    run_test(
        test_graph! {
            0(5, 1)
            1(2, 3)
            2(4, 3)
            3(4)
            4(9)
            5(6, 7)
            6(8, 7)
            7(8)
            8(9)
            9()
        },
        test_graph! {
            0(1)
            1(2)
            2(3.dot)
            3(4.dot)
            4(5.dot)
            5(6)
            6(7.dot)
            7(8.dot)
            8(9)
            9()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | 3            3
| |/|            :
| 4 |    ->      4
| 5 |            5
| | |            :
6 | |            6
|/ /             |
7 /              7
|/               :
8                8
 */
#[test]
fn interesting_test() {
    run_test(
        test_graph! {
            0(6, 1)
            1(4, 2)
            2(3)
            3(4, 8)
            4(5)
            5(7)
            6(7)
            7(8)
            8()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4.dot)
            4(5)
            5(6.dot)
            6(7)
            7(8.dot)
            8()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | |\           |
| | | 3          3
| | | |          :
| | 4 |          4
| |/  |          :
| 5   |    ->    5
| 6   |          6
| |  /           :
7 | /            7
|/ /             |
8 /              8
|/               |
9                9
 */
#[test]
fn interesting_test2() {
    run_test(
        test_graph! {
            0(7, 1)
            1(5, 2)
            2(4, 3)
            3(9)
            4(5)
            5(6)
            6(8)
            7(8)
            8(9)
            9()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4.dot)
            4(5.dot)
            5(6)
            6(7.dot)
            7(8)
            8(9)
            9()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | |            :
3 | |    ->      3
4 | |            4
|/ /             |
5 /              5
\/               |
6                6
 */
#[test]
fn small_recursive_branches_test() {
    run_test(
        test_graph! {
            0(3, 1)
            1(5, 2)
            2(6)
            3(4)
            4(5)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2)
            2(3.dot)
            3(4)
            4(5)
            5(6)
            6()
        },
    )
}

/*
0                0
|\               |
| 1              1
| 2              2
| |\             |\
| | 3            | 3
| | 4            | 4
| | |\           | 5
| | | \          | :
| | | 5    ->    6 :
| 6 | |          7 :
| 7 | |          :'
8 |/  |          8
|/|   |          |
9 |  /           9
|/  /            |
10 /             10
11               11
 */
#[test]
fn two_tails_branch_test() {
    run_test(
        test_graph! {
            0(8, 1)
            1(2)
            2(6, 3)
            3(4)
            4(9, 5)
            5(11)
            6(7)
            7(10)
            8(9)
            9(10)
            10(11)
            11()
        },
        test_graph! {
            0(1)
            1(2)
            2(6, 3)
            3(4)
            4(5)
            5(8.dot)
            6(7)
            7(8.dot)
            8(9)
            9(10)
            10(11)
            11()
        },
    )
}

/*
0                0
|\   1           |  1
| 2 |            2 /
| |/             |/
| 3       ->     3
| |              :
4 |              4
|/               |
5                5
 */
#[test]
fn incoming_edge_test() {
    run_test(
        test_graph! {
            0(4, 2)
            1(3)
            2(3)
            3(5)
            4(5)
            5()
        },
        test_graph! {
            0(2)
            1(3)
            2(3)
            3(4.dot)
            4(5)
            5()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             :
2 ||             2
|/ |             |
3  |     ->      3
|\ |             :
| \|             :
| 4              4
| |              :
5 |              5
|/               |
6                6
 */
#[test]
fn hidden_incoming_edges() {
    run_test(
        test_graph! {
            0(2, 1)
            1(3, 4)
            2(3)
            3(5, 4)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2(3)
            3(4.dot)
            4(5.dot)
            5(6)
            6()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | 3            3
| | 4            4
| |/             :
|/               :
5                5
6                6
7                7
 */
#[test]
fn test_merge_with_old_commit() {
    run_test(
        test_graph! {
            0(5, 1)
            1(5, 2)
            2(3)
            3(4)
            4(5)
            5(6)
            6(7)
            7()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4)
            4(5.dot)
            5(6)
            6(7)
            7()
        },
    )
}

/*
0                0
|   1            | 1
2   |            2 |
|\  |            | |
| 3 |     ->     3 |
| |/             |/
| 4              4
|/               :
5                5
*/
#[test]
fn test_heads_order1() {
    run_test(
        test_graph! {
            0(2)
            1(4)
            2(5, 3)
            3(4)
            4(5)
            5()
        },
        test_graph! {
            0(2)
            1(4)
            2(3)
            3(4)
            4(5.dot)
            5()
        },
    )
}

/*
    0               0
1   |           1   |
2   |           2   |
|\  |           |   |
| 3 |     ->    3  /
| |/            |/
| 4             4
|/              :
5               5
*/
#[test]
fn test_heads_order2() {
    run_test(
        test_graph! {
            0(4)
            1(2)
            2(5, 3)
            3(4)
            4(5)
            5()
        },
        test_graph! {
            0(4)
            1(2)
            2(3)
            3(4)
            4(5.dot)
            5()
        },
    )
}

/*
0                0
|\               |
| 1              1
| 2      ->      2
|/               :
3                3
*/
#[test]
fn test_katisha() {
    run_test(
        test_graph! {
            0(1, 3)
            1(2)
            2(3)
            3()
        },
        test_graph! {
            0(1)
            1(2)
            2(3.dot)
            3()
        },
    )
}

/*
0                0
|\               |\
| 1      ->      | 1
| 2              | 2
3                3
 */
#[test]
fn test_initial_import() {
    run_test(
        test_graph! {
            0(3, 1)
            1(2)
            2()
            3()
        },
        test_graph! {
            0(3, 1)
            1(2)
            2()
            3()
        },
    )
}

/*
0              0
|\             |
| 1    ->      1
|/             :
2              2
 */
#[test]
fn test_triangle() {
    run_test(
        test_graph! {
            0(2, 1)
            1(2)
            2()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2()
        },
    )
}

/*
0
|\
1 \
2 \
|\ \
3 \|
|\ 4
| \|
| 5
|/
6
 */
#[test]
fn reversed_parents_test() {
    run_test(
        test_graph! {
            0(1, 4)
            1(2)
            2(3, 4)
            3(5, 6)
            4(5)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4.dot)
            4(5)
            5(6.dot)
            6()
        },
    )
}

/*
0               0
|\              |
| 1             1
| |\            |\
| | 2           | 2
| 3 |    -->    3 :
| \'            : :
|/|             :'
4 |             4
|/              |
5               5
 */
#[test]
fn crossed_tails_test() {
    run_test(
        test_graph! {
            0(4, 1)
            1(3, 2)
            2(4)
            3(5)
            4(5)
            5()
        },
        test_graph! {
            0(1)
            1(3, 2)
            2(4.dot)
            3(4.dot)
            4(5)
            5()
        },
    )
}
