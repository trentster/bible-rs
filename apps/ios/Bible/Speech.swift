import AVFoundation
import CryptoKit
import Foundation
import Observation

/// Read Aloud on the phone.
///
/// The words, the trimming, the pace and the gaps all come from the shared engine in `bible-core`,
/// so the phone and the Mac say the same thing the same way. What is the phone's own is the two
/// pieces that cannot be shared: the voice is fetched with Apple's `URLSession` rather than the
/// Mac's `ureq`, so the iOS build carries no encryption code of its own, and the samples are played
/// with Apple's audio stack rather than CoreAudio directly.
///
/// One rule from the Mac carries over exactly: the voice follows the page. Anything that moves the
/// reader stops the reading.
@MainActor
@Observable
final class Speaker {
    /// How long a reading keeps going. The last choice is remembered per device.
    enum Stop: String, CaseIterable, Identifiable {
        case endOfChapter, fiveMinutes, fifteenMinutes, thirtyMinutes, fortyFiveMinutes
        case sixtyMinutes, twoHours, keepReading

        var id: String { rawValue }

        var title: String {
            switch self {
            case .endOfChapter: "End of chapter"
            case .fiveMinutes: "5 minutes"
            case .fifteenMinutes: "15 minutes"
            case .thirtyMinutes: "30 minutes"
            case .fortyFiveMinutes: "45 minutes"
            case .sixtyMinutes: "60 minutes"
            case .twoHours: "2 hours"
            case .keepReading: "Keep reading"
            }
        }

        /// How long it runs, or `nil` when there is no clock: the end of the chapter, or for ever.
        var seconds: TimeInterval? {
            switch self {
            case .endOfChapter, .keepReading: nil
            case .fiveMinutes: 5 * 60
            case .fifteenMinutes: 15 * 60
            case .thirtyMinutes: 30 * 60
            case .fortyFiveMinutes: 45 * 60
            case .sixtyMinutes: 60 * 60
            case .twoHours: 2 * 60 * 60
            }
        }
    }

    enum Phase: Equatable {
        case off
        case downloading(Double)
        case preparing
        case reading
        case paused
        case failed(String)

        var isOn: Bool { self != .off }
    }

    private(set) var phase: Phase = .off
    /// The verse being said right now, so the page can follow it.
    private(set) var speaking: UInt8?
    /// The chapter the reading is in, which may not be the one on screen once it carries on.
    private(set) var chapter: Int?

    var voice: String = UserDefaults.standard.string(forKey: "readAloudVoice") ?? "Russell" {
        didSet { UserDefaults.standard.set(voice, forKey: "readAloudVoice") }
    }

    var stopAfter: Stop = Stop(rawValue: UserDefaults.standard.string(forKey: "readAloudStop") ?? "") ?? .endOfChapter {
        didSet { UserDefaults.standard.set(stopAfter.rawValue, forKey: "readAloudStop") }
    }

    private var engine: AVAudioEngine?
    private var player: AVAudioPlayerNode?
    private var format: AVAudioFormat?
    private var synth: Synth?
    private var work: Task<Void, Never>?
    private var utterances: [SpokenUtterance] = []
    private var next = 0
    private var deadline: Date?
    /// Fetches a chapter's utterances and says where it is, so Keep reading can walk on.
    private var provider: ((Int) -> (verses: [SpokenUtterance], chapter: Int))?

    /// Where an installed voice lives: the app's own container.
    static var directory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("speech", isDirectory: true)
    }

    var installed: Bool { voiceComplete(dir: Self.directory.path) }

    /// The voices offered, with the name a reader should see.
    var offered: [VoiceInfo] { voices() }

    // MARK: - Reading

    /// Starts at `chapterIndex`, saying its verses from the first word. `provider` is what lets the
    /// reading carry on into the next chapter when the timer is not the thing stopping it.
    func start(chapterIndex: Int, provider: @escaping (Int) -> (verses: [SpokenUtterance], chapter: Int)) {
        self.provider = provider
        let first = provider(chapterIndex)
        guard !first.verses.isEmpty else {
            phase = .failed("there is nothing here to read")
            return
        }
        chapter = first.chapter
        utterances = first.verses
        next = 0
        deadline = stopAfter.seconds.map { Date().addingTimeInterval($0) }
        phase = .preparing
        work?.cancel()
        work = Task { await run() }
    }

    func pause() {
        guard phase == .reading else { return }
        player?.pause()
        phase = .paused
    }

    func resume() {
        guard phase == .paused else { return }
        player?.play()
        phase = .reading
    }

    /// Stops and takes the reading down: the page moved, or the reader asked it to.
    func stop() {
        work?.cancel()
        work = nil
        player?.stop()
        engine?.stop()
        engine = nil
        player = nil
        format = nil
        synth = nil
        utterances = []
        next = 0
        speaking = nil
        chapter = nil
        deadline = nil
        phase = .off
    }

    // MARK: - The loop

    private func run() async {
        while next < utterances.count, !Task.isCancelled {
            if let deadline, Date() >= deadline {
                // A timed reading finishes the sentence it is in the middle of, then stops.
                break
            }
            let utterance = utterances[next]
            next += 1
            if let verse = utterance.verse { speaking = verse }
            let text = utterance.text
            let voice = voice
            let synth = await openVoice()
            guard let synth, !Task.isCancelled else { return }
            let pcm: [Float]
            do {
                pcm = try await Task.detached(priority: .userInitiated) {
                    try synth.say(text, voice)
                }.value
            } catch {
                phase = .failed("\(error)")
                return
            }
            guard !Task.isCancelled else { return }
            phase = .reading
            await play(pcm)
            if utterance.pauseMs > 0, !Task.isCancelled {
                try? await Task.sleep(nanoseconds: UInt64(utterance.pauseMs) * 1_000_000)
            }
            if phase == .paused {
                // A pause holds the reading where it is until the reader comes back.
                while phase == .paused, !Task.isCancelled {
                    try? await Task.sleep(nanoseconds: 100_000_000)
                }
            }
        }
        guard !Task.isCancelled else { return }
        // The chapter is done. Keep reading walks on; anything else stops with the chapter.
        if stopAfter == .keepReading, let chapter, let provider {
            let following = provider(chapter + 1)
            if !following.verses.isEmpty {
                self.chapter = following.chapter
                utterances = following.verses
                next = 0
                await run()
                return
            }
        }
        stop()
    }

    /// Opens the model once and keeps it for the session.
    private func openVoice() async -> Synth? {
        if let synth { return synth }
        phase = .preparing
        do {
            try prepareAudio()
        } catch {
            phase = .failed("the speaker could not start: \(error.localizedDescription)")
            return nil
        }
        do {
            // Read the path here: `directory` is main-actor state and the model opens off it.
            let path = Self.directory.path
            let made = try await Task.detached(priority: .userInitiated) {
                Synth(try Speech.load(dir: path))
            }.value
            synth = made
            return made
        } catch {
            phase = .failed("the voice could not start: \(error)")
            return nil
        }
    }

    private func prepareAudio() throws {
        guard engine == nil else { return }
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.playback, mode: .spokenAudio)
        try session.setActive(true)
        let engine = AVAudioEngine()
        let player = AVAudioPlayerNode()
        engine.attach(player)
        // The engine says 24 kHz; ask for it rather than assuming, so a change upstream is loud.
        let rate = synth?.rate ?? 24_000
        guard let format = AVAudioFormat(standardFormatWithSampleRate: Double(rate), channels: 1) else {
            throw SpeakerError.noFormat
        }
        engine.connect(player, to: engine.mainMixerNode, format: format)
        try engine.start()
        player.play()
        self.engine = engine
        self.player = player
        self.format = format
    }

    private func play(_ samples: [Float]) async {
        guard !samples.isEmpty, let player, let format else { return }
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(samples.count)) else {
            return
        }
        buffer.frameLength = AVAudioFrameCount(samples.count)
        samples.withUnsafeBufferPointer { source in
            buffer.floatChannelData?[0].update(from: source.baseAddress!, count: samples.count)
        }
        await withCheckedContinuation { (done: CheckedContinuation<Void, Never>) in
            player.scheduleBuffer(buffer) { done.resume() }
        }
    }

    // MARK: - The voice download

    /// Fetches the voice through `URLSession`, checking every file against its pinned digest before
    /// it is kept. One file at a time, so progress is honest on a slow line.
    func download(progress: @escaping @MainActor (Double) -> Void) async {
        let files = voiceFiles()
        let base = voiceBundleUrl()
        phase = .downloading(0)
        let manager = FileManager.default
        do {
            try manager.createDirectory(at: Self.directory, withIntermediateDirectories: true)
            var done = 0.0
            for file in files {
                let dest = Self.directory.appendingPathComponent(file.name)
                if manager.fileExists(atPath: dest.path), (try? Self.digest(of: dest)) == file.sha256 {
                    done += 1
                    progress(done / Double(files.count))
                    continue
                }
                guard let url = URL(string: "\(base)/\(file.name)") else { throw SpeakerError.badURL }
                let (tmp, response) = try await URLSession.shared.download(from: url)
                if let http = response as? HTTPURLResponse, http.statusCode != 200 {
                    throw SpeakerError.badResponse(file.name, http.statusCode)
                }
                guard try Self.digest(of: tmp) == file.sha256 else {
                    throw SpeakerError.wrongDigest(file.name)
                }
                try? manager.removeItem(at: dest)
                try manager.moveItem(at: tmp, to: dest)
                done += 1
                progress(done / Double(files.count))
            }
            phase = .off
        } catch {
            phase = .failed("the voice could not be downloaded: \(error)")
        }
    }

    static func digest(of url: URL) throws -> String {
        let data = try Data(contentsOf: url, options: .mappedIfSafe)
        return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

/// The engine object, handed to a background task. `Speech` wraps a lock of its own, so a single
/// synthesis at a time is safe off the main actor; this only spares the Swift compiler from proving
/// something the Rust side already guarantees.
private final class Synth: @unchecked Sendable {
    private let speech: Speech

    init(_ speech: Speech) {
        self.speech = speech
    }

    /// 24 kHz mono samples, at the pace the approved voices are calibrated to.
    var rate: UInt32 { speech.sampleRate() }

    func say(_ text: String, _ voice: String) throws -> [Float] {
        try speech.say(text: text, voiceName: voice)
    }
}

enum SpeakerError: LocalizedError {
    case noFormat
    case badURL
    case badResponse(String, Int)
    case wrongDigest(String)

    var errorDescription: String? {
        switch self {
        case .noFormat: "the speaker could not be set up"
        case .badURL: "the voice's address is wrong"
        case .badResponse(let name, let code): "\(name) came back as \(code)"
        case .wrongDigest(let name): "\(name) arrived damaged"
        }
    }
}
