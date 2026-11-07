import SwiftUI

/// Read Aloud's own panel: the voice, how long it reads for, and the one-time download.
///
/// The same eight choices as the Mac, in the same order, remembered per device. Nothing here is
/// needed before the first press of Listen: the panel opens on the download when the voice is not
/// there yet, and on the reading controls once it is.
struct ListenSheet: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss

    @State private var progress: Double = 0
    @State private var downloading = false

    private var speaker: Speaker { store.speaker }

    var body: some View {
        NavigationStack {
            List {
                if !speaker.installed || downloading {
                    downloadSection
                } else {
                    readSection
                    voiceSection
                    timerSection
                }
            }
            .navigationTitle("Read Aloud")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    // MARK: - Sections

    private var downloadSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 12) {
                Text("The reading voice downloads once, when you ask.")
                    .font(.subheadline)
                Text("About 66 MB. Nothing else in Lamplit ever uses the internet.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                if downloading {
                    ProgressView(value: progress) {
                        Text("\(Int(progress * 100))%")
                            .font(.caption.monospacedDigit())
                    }
                } else {
                    Button {
                        Task { await download() }
                    } label: {
                        Label("Download the voice", systemImage: "arrow.down.circle")
                    }
                }
                if case .failed(let why) = speaker.phase {
                    Text(why).font(.footnote).foregroundStyle(.red)
                }
            }
            .padding(.vertical, 4)
        } footer: {
            Text("The voice is fetched from the project's own release page and checked, file by file, on the way in.")
        }
    }

    private var readSection: some View {
        Section {
            Button {
                store.toggleReading()
                if speaker.phase.isOn { dismiss() }
            } label: {
                Label(speaker.phase.isOn ? "Stop reading" : "Start reading",
                      systemImage: speaker.phase.isOn ? "stop.fill" : "play.fill")
            }
            if let verse = speaker.speaking, let chapter = speaker.chapter {
                LabeledContent("Speaking") {
                    Text("\(chapterLabel(index: UInt16(chapter))) :\(verse)")
                }
            }
        } footer: {
            Text("The voice follows the page: choosing a chapter, a verse or another translation stops the reading.")
        }
    }

    private var voiceSection: some View {
        Section("Voice") {
            ForEach(speaker.offered, id: \.name) { voice in
                Button {
                    speaker.voice = voice.name
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(voice.name)
                            Text("\(voice.accent) · \(voice.female ? "female" : "male") · \(voice.nativeWpm) wpm")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if speaker.voice == voice.name {
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        }
    }

    private var timerSection: some View {
        Section {
            Picker("Read for", selection: Binding(
                get: { speaker.stopAfter },
                set: { speaker.stopAfter = $0 }
            )) {
                ForEach(Speaker.Stop.allCases) { choice in
                    Text(choice.title).tag(choice)
                }
            }
        } footer: {
            Text("A reading that runs out of time finishes the sentence it is in, then stops.")
        }
    }

    // MARK: - Downloading

    private func download() async {
        downloading = true
        progress = 0
        await speaker.download { done in
            progress = done
        }
        downloading = false
    }
}
