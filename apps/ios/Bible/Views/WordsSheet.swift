import SwiftUI

/// The verse in the language it was written in: each word with how it sounds and the gloss
/// translators give it. Tapping a word opens the dictionary entry behind its number.
///
/// The Mac offers this from one word in the text; a phone has no room for that, so it lives on the
/// verse you have tapped, in the bar, only when there is original text to show.
struct WordsSheet: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let ref: VerseRef
    let theme: Theme

    var body: some View {
        let verse = store.original(ref)
        NavigationStack {
            Group {
                if let verse, !verse.words.isEmpty {
                    List {
                        Section {
                            ForEach(Array(verse.words.enumerated()), id: \.offset) { _, word in
                                NavigationLink {
                                    EntryView(word: word, theme: theme)
                                } label: {
                                    wordRow(word)
                                }
                            }
                        } footer: {
                            Text(store.wordsCreditLine)
                                .font(.caption)
                                .foregroundStyle(Color(theme.ink3))
                        }
                    }
                    .listStyle(.insetGrouped)
                } else {
                    ContentUnavailableView {
                        Label("No original text", systemImage: "text.book.closed")
                    } description: {
                        Text("We have no Hebrew or Greek for \(label).")
                    }
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    private var label: String { rangeLabel(from: ref, to: ref) }

    private var title: String {
        guard let language = store.original(ref)?.language else { return label }
        return "\(language) · \(label)"
    }

    private func wordRow(_ word: OriginalWord) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(word.surface)
                .font(.system(size: 24))
                .foregroundStyle(Color(theme.ink))
            VStack(alignment: .leading, spacing: 2) {
                Text(word.translit)
                    .font(.subheadline)
                    .foregroundStyle(Color(theme.ink2))
                Text(word.gloss)
                    .font(.footnote)
                    .foregroundStyle(Color(theme.ink3))
                    .lineLimit(2)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
    }
}

/// One lexicon entry: the number, the word, and its definition line by line.
private struct EntryView: View {
    @Environment(Store.self) private var store
    let word: OriginalWord
    let theme: Theme

    var body: some View {
        let entry = store.entry(word.strong)
        List {
            if let entry {
                Section {
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                            Text(entry.lemma)
                                .font(.system(size: 28))
                                .foregroundStyle(Color(theme.ink))
                            Text(entry.translit)
                                .font(.subheadline)
                                .foregroundStyle(Color(theme.ink2))
                        }
                        Text(entry.short)
                            .font(.body)
                            .foregroundStyle(Color(theme.ink))
                        Text("\(entry.id) · \(entry.pos)")
                            .font(.caption)
                            .foregroundStyle(Color(theme.ink3))
                    }
                    .padding(.vertical, 4)
                }
                Section {
                    ForEach(Array(entry.lines.enumerated()), id: \.offset) { _, line in
                        Text(line)
                            .font(.callout)
                            .foregroundStyle(Color(theme.ink2))
                    }
                }
            } else {
                Section {
                    Text("No entry for \(word.strong)")
                        .foregroundStyle(Color(theme.ink2))
                } footer: {
                    Text(store.wordsCreditLine)
                        .font(.caption)
                        .foregroundStyle(Color(theme.ink3))
                }
            }
        }
        .listStyle(.insetGrouped)
        .navigationTitle(word.strong)
        .navigationBarTitleDisplayMode(.inline)
    }
}
