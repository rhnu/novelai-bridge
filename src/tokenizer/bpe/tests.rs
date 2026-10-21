use super::MergeTable;

fn merged(table: &MergeTable, text: &str) -> Vec<String> {
    table
        .merge(
            text.chars()
                .map(|symbol| table.symbol(symbol.to_string()))
                .collect(),
        )
        .into_iter()
        .map(|symbol| symbol.text)
        .collect()
}

#[test]
fn repeated_equal_rank_pairs_merge_leftmost_first() {
    let table = MergeTable::from_ranked_pairs([("a", "a"), ("a", "aa"), ("aa", "aa")]);
    assert_eq!(merged(&table, "aaa"), ["aa", "a"]);
    assert_eq!(merged(&table, "aaaaa"), ["aaaa", "a"]);
}

#[test]
fn changed_neighbors_replace_stale_merge_candidates() {
    let table = MergeTable::from_ranked_pairs([("a", "bc"), ("b", "c"), ("a", "b"), ("c", "d")]);
    assert_eq!(merged(&table, "abcd"), ["abc", "d"]);

    let table = MergeTable::from_ranked_pairs([("ab", "c"), ("a", "b"), ("b", "c")]);
    assert_eq!(merged(&table, "abc"), ["abc"]);
}

#[test]
fn empty_single_and_unknown_unicode_symbols_survive() {
    let table = MergeTable::from_ranked_pairs([("猫", "🙂"), ("猫🙂", "猫")]);
    assert_eq!(merged(&table, ""), Vec::<String>::new());
    assert_eq!(merged(&table, "é"), ["é"]);
    assert_eq!(merged(&table, "é🙂é"), ["é", "🙂", "é"]);
    assert_eq!(merged(&table, "é猫🙂猫é猫🙂"), ["é", "猫🙂猫", "é", "猫🙂"]);
}

#[test]
fn repeated_merge_definitions_keep_the_earliest_rank() {
    let table = MergeTable::from_ranked_pairs([("a", "b"), ("b", "c"), ("a", "b")]);
    assert_eq!(merged(&table, "abc"), ["ab", "c"]);
}

#[test]
fn long_repeated_word_retains_the_unmerged_tail() {
    let mut symbol = "a".to_owned();
    let mut pairs = Vec::new();
    for _ in 0..12 {
        pairs.push((symbol.clone(), symbol.clone()));
        symbol = symbol.repeat(2);
    }
    let table = MergeTable::from_ranked_pairs(pairs);
    assert_eq!(merged(&table, &"a".repeat(4097)), [symbol, "a".to_owned()]);
}
