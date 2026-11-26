import SwiftUI

extension CardDto {
    var hasProvenance: Bool {
        !(rationale ?? "").isEmpty || !sources.isEmpty || !(proposedBy ?? "").isEmpty
    }
}

/// Shared provenance renderer used by both the revealed review card disclosure
/// and the Card Info inspector sheet.
struct CardProvenanceView: View {
    enum Style {
        case disclosure
        case formRows
    }

    let card: CardDto
    let style: Style

    var body: some View {
        switch style {
        case .disclosure:
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 8) {
                    if let rationale = card.rationale, !rationale.isEmpty {
                        Text(rationale)
                            .font(.footnote)
                            .foregroundStyle(RemColor.text)
                    }
                    ForEach(Array(card.sources.enumerated()), id: \.offset) { _, source in
                        sourceRow(source, labelColor: RemColor.muted)
                    }
                    if let proposedBy = card.proposedBy, !proposedBy.isEmpty {
                        Text("Proposed by \(proposedBy)")
                            .font(.caption)
                            .foregroundStyle(RemColor.faint)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, 6)
            } label: {
                Text("Source & rationale")
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(RemColor.muted)
            }
            .tint(RemColor.muted)

        case .formRows:
            if let rationale = card.rationale, !rationale.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Rationale")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(RemColor.muted)
                    Text(rationale)
                        .font(.subheadline)
                        .foregroundStyle(RemColor.text)
                }
                .padding(.vertical, 2)
            }
            ForEach(Array(card.sources.enumerated()), id: \.offset) { _, source in
                sourceRow(source, labelColor: RemColor.text)
                    .padding(.vertical, 2)
            }
            if let proposedBy = card.proposedBy, !proposedBy.isEmpty {
                LabeledContent("Proposed by", value: proposedBy)
            }
        }
    }

    private func sourceRow(_ source: DraftSourceDto, labelColor: Color) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            if let label = source.label, !label.isEmpty {
                Text(label)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(labelColor)
            }
            Text(source.locator)
                .font(.caption.monospaced())
                .foregroundStyle(RemColor.muted)
                .textSelection(.enabled)
        }
    }
}
