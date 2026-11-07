import SwiftUI

/// The verses the reading tradition links to this one, best-voted first, with the data's credit at
/// the foot — the same order and the same credit as the card on the Mac. Tapping one goes there.
struct RelatedSheet: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let ref: VerseRef
    let theme: Theme

    var body: some View {
        let links = store.related(ref)
        NavigationStack {
            Group {
                if links.isEmpty {
                    ContentUnavailableView {
                        Label("No related verses", systemImage: "link")
                    } description: {
                        Text("The tradition links nothing to \(label).")
                    }
                } else {
                    List {
                        Section {
                            ForEach(Array(links.enumerated()), id: \.offset) { _, link in
                                Button {
                                    open(link)
                                } label: {
                                    row(link)
                                }
                                .buttonStyle(.plain)
                            }
                        } footer: {
                            Text(store.relatedCreditLine)
                                .font(.caption)
                                .foregroundStyle(Color(theme.ink3))
                        }
                    }
                    .listStyle(.insetGrouped)
                }
            }
            .navigationTitle("Related to \(label)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    private var label: String { rangeLabel(from: ref, to: ref) }

    private func row(_ link: RelatedLink) -> some View {
        HStack(spacing: 10) {
            Text(link.label)
                .font(.body.weight(.medium))
                .foregroundStyle(Color(theme.ink))
            Spacer(minLength: 0)
            Image(systemName: "chevron.right")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(theme.ink3))
        }
        .contentShape(Rectangle())
    }

    private func open(_ link: RelatedLink) {
        if let index = store.chapterIndex(book: link.book, chapter: link.chapter) {
            store.go(to: index, verse: Int(link.verse), flash: true)
        }
        dismiss()
    }
}
