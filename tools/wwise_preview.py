"""Read the one-shot random containers used by Mortal's Wwise v140 SFX bank.

Layout evidence: bnnm/wwiser's parser/wparser.py (v140 branches), checked
against a local bank dump. This is deliberately not a general Wwise renderer:
state-dependent, layered, continuous, or DSP-driven events remain unsupported.
"""
import math
import struct


UNSUPPORTED = "此音频事件使用了尚不支持的动态播放规则，请在游戏中确认"


class Reader:
    def __init__(self, data):
        self.data, self.at = data, 0

    def read(self, fmt):
        size = struct.calcsize('<' + fmt)
        if self.at + size > len(self.data):
            raise ValueError("音频播放规则不完整")
        values = struct.unpack_from('<' + fmt, self.data, self.at)
        self.at += size
        return values[0] if len(values) == 1 else values

    def end(self):
        if self.at != len(self.data):
            raise ValueError(UNSUPPORTED)


def require(condition):
    if not condition:
        raise ValueError(UNSUPPORTED)


def objects_at(bank):
    objects, version = {}, None
    with bank.open('rb') as stream:
        while header := stream.read(8):
            if len(header) != 8:
                raise ValueError("音频包区块头不完整")
            tag, size = struct.unpack('<4sI', header)
            if tag not in (b'BKHD', b'HIRC'):
                stream.seek(size, 1)
                continue
            data = stream.read(size)
            if len(data) != size:
                raise ValueError("音频包播放规则不完整")
            reader = Reader(data)
            if tag == b'BKHD':
                version = reader.read('I')
            else:
                for _ in range(reader.read('I')):
                    kind, length = reader.read('BI')
                    if length < 4 or reader.at + length > len(data):
                        raise ValueError("音频包对象范围无效")
                    payload = data[reader.at:reader.at + length]
                    reader.at += length
                    ident = struct.unpack_from('<I', payload)[0]
                    if ident in objects:
                        raise ValueError("音频包对象编号重复")
                    objects[ident] = (kind, payload)
                reader.end()
    require(version == 140)
    return objects


def base_parameters(reader):
    # FX slots, metadata slots and attachment overrides are absent in this profile.
    override, count = reader.read('BB')
    require(override in (0, 1) and count == 0)
    override, count = reader.read('BB')
    require(override in (0, 1) and count == 0)
    require(reader.read('B') == 0)
    reader.read('I')  # Output bus; in-game mixer/user-volume is not a preview setting.
    parent = reader.read('I')
    reader.read('B')  # Priority/MIDI flags do not alter these one-shot samples.
    count = reader.read('B')
    ids = [reader.read('B') for _ in range(count)]
    require(len(set(ids)) == count and set(ids) <= {0, 2})  # Volume dB, pitch cents.
    values = {key: reader.read('f') for key in ids}
    count = reader.read('B')
    ids = [reader.read('B') for _ in range(count)]
    require(len(set(ids)) == count and set(ids) <= {0, 2})
    ranges = {key: reader.read('ff') for key in ids}
    require(all(math.isfinite(value) for value in values.values()))
    require(all(math.isfinite(low) and math.isfinite(high) and low <= high
                for low, high in ranges.values()))
    positioning = reader.read('B')
    require(positioning in (0, 3))
    if positioning == 3:
        require(reader.read('B') == 8)  # Non-spatial, no automation/attenuation curve.
    require(reader.read('B') == 0)  # No auxiliary sends.
    require(reader.read('I') == 0)  # No reflection bus.
    reader.read('BBHB')  # Virtual voice/instance limits do not affect an isolated trigger.
    require(reader.read('B') == 0)  # No loudness normalization/envelope.
    require(reader.read('B') == 0)  # No state-property curves.
    require(reader.read('B') == 0)  # No state groups.
    require(reader.read('H') == 0)  # No RTPC-dependent pitch/volume/filtering.
    return parent, values, ranges


def random_choices(bank, event_id, declared_media):
    """Return every playable branch, preserving duplicate-media pitch variations."""
    objects = objects_at(bank)

    def node(ident, expected):
        if ident not in objects:
            raise ValueError("音频事件引用的播放对象不存在")
        kind, payload = objects[ident]
        require(kind == expected)
        reader = Reader(payload)
        require(reader.read('I') == ident)
        return reader

    event = node(event_id, 4)
    require(event.read('B') == 1)  # A single Play action, not a layered event.
    action_id = event.read('I')
    event.end()
    action = node(action_id, 3)
    require(action.read('H') == 0x0403)
    container_id = action.read('I')
    require(action.read('BBB') == (0, 0, 0))  # No bus target, delay, or randomized action params.
    require(action.read('B') == 4)  # Linear/no-fade Play.
    action.read('I')  # Referenced bank ID.
    action.end()
    container = node(container_id, 5)
    random_id = container_id
    inherited = [base_parameters(container)]
    require(container.read('HHH') == (1, 0, 0))  # Play exactly once.
    container.read('fff')  # Transition timings are unused when transitions are disabled.
    container.read('H')  # Avoid-repeat history starts empty for this isolated preview.
    transition, random_mode, mode, flags = container.read('BBBB')
    require(transition == 0 and random_mode in (0, 1) and mode == 0 and flags & 8 == 0)
    children = [container.read('I') for _ in range(container.read('I'))]
    playlist = [container.read('Ii') for _ in range(container.read('H'))]
    container.end()
    require(bool(playlist) and {ident for ident, _ in playlist} == set(children))
    require(all(weight >= 0 for _, weight in playlist))
    parent = inherited[0][0]
    visited = {container_id}
    while parent:
        require(parent not in visited)
        visited.add(parent)
        mixer = node(parent, 7)
        parameters = base_parameters(mixer)
        require(container_id in [mixer.read('I') for _ in range(mixer.read('I'))])
        mixer.end()
        inherited.append(parameters)
        container_id, parent = parent, parameters[0]
    choices = []
    for ident, weight in playlist:
        sound = node(ident, 2)
        require(sound.read('I') & 0xF == 1)  # Codec source, not a synth/plugin.
        require(sound.read('B') == 0)  # Embedded media.
        media_id = sound.read('I')
        sound.read('I')
        require(sound.read('B') == 0)
        parameters = base_parameters(sound)
        sound.end()
        require(parameters[0] == random_id and media_id in declared_media)
        choices.append({
            'media_id': media_id,
            'weight': weight if flags & 1 else 1,
            'parameters': inherited + [parameters],
        })
    require({choice['media_id'] for choice in choices} == declared_media)
    require(any(choice['weight'] > 0 for choice in choices))
    return choices


def playback_parameters(choice, rng):
    volume, pitch = 0.0, 0.0
    for _, values, ranges in choice['parameters']:
        volume += values.get(0, 0.0)
        pitch += values.get(2, 0.0)
        if 0 in ranges:
            volume += rng.uniform(*ranges[0])
        if 2 in ranges:
            pitch += rng.uniform(*ranges[2])
    return volume, pitch


def write_playback_config(output, volume, pitch):
    """Use the bundled vgmstream TXTP renderer for gain and playback-rate changes."""
    data = output.read_bytes()
    require(data[:4] == b'RIFF')
    reader = Reader(data[12:])
    rate = None
    while reader.at < len(reader.data):
        tag, size = reader.read('4sI')
        if tag == b'fmt ':
            require(size >= 16)
            reader.read('HH')
            rate = reader.read('I')
            break
        reader.at += size + (size & 1)
    require(rate is not None and rate > 0)
    gain = 10 ** (volume / 20)
    adjusted = round(rate * 2 ** (pitch / 1200))
    require(math.isfinite(gain) and gain > 0 and adjusted > 0)
    output.with_suffix('.txtp').write_text(
        f'{output.name} #i #v {gain:.9f} #h {adjusted}\n', encoding='utf-8')
