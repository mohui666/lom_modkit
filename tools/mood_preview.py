"""Read native mood mappings and animation keyframes from the user's game."""
import struct

import UnityPy
from PIL import Image

from _probe_assets import _read_str


def animation_frames(clip):
    """Decode the single sprite-index curve retained in the built AnimationClip."""
    stream = clip.m_MuscleClip.m_Clip.data.m_StreamedClip
    if stream.curveCount != 1:
        raise ValueError("气泡动画曲线结构已改变")
    raw = struct.pack(f"<{len(stream.data)}I", *stream.data)
    offset = 0
    frames = []
    sprites = clip.m_ClipBindingConstant.pptrCurveMapping
    while offset < len(raw):
        time, count = struct.unpack_from("<fi", raw, offset)
        offset += 8
        for _ in range(count):
            curve, _, _, _, value = struct.unpack_from("<i4f", raw, offset)
            offset += 20
            if curve != 0 or not value.is_integer() or not 0 <= value < len(sprites):
                raise ValueError("气泡动画包含未知精灵帧")
            frames.append((max(0.0, time), sprites[int(value)]))
    if not frames:
        raise ValueError("气泡动画没有精灵帧")
    return frames


def extract_moods(data, output):
    environment = UnityPy.load(str(data / "sharedassets2.assets"))
    panels = []
    for obj in environment.objects:
        if obj.type.name == "MonoBehaviour":
            script = obj.read(check_read=False).m_Script.read()
            if script.m_ClassName == "CharacterMoodPanel":
                panels.append(obj)
    if len(panels) != 1:
        raise ValueError("无法唯一定位原版 CharacterMoodPanel")
    panel = panels[0]
    objects = panel.assets_file.objects
    raw = panel.get_raw_data()
    _, offset = _read_str(raw, 28)
    count = struct.unpack_from("<i", raw, offset)[0]
    offset += 4
    animations = {}

    def animation(pid):
        if pid in animations:
            return animations[pid]
        game_object = objects[pid].read()
        components = {entry.component.type.name: entry.component for entry in game_object.m_Component}
        rect = components["RectTransform"].read()
        controller = components["Animator"].read().m_Controller.read()
        machine = controller.m_Controller.m_StateMachineArray[0].data
        state_index = machine.m_DefaultState
        clips = []
        visited = set()
        while state_index not in visited:
            visited.add(state_index)
            state = machine.m_StateConstantArray[state_index].data
            trees = state.m_BlendTreeConstantArray
            if len(trees) != 1 or len(trees[0].data.m_NodeArray) != 1 or state.m_Speed != 1:
                raise ValueError("气泡 Animator 使用了未知混合或速度")
            clip_id = trees[0].data.m_NodeArray[0].data.m_ClipID
            clips.append(controller.m_AnimationClips[clip_id].read())
            transitions = state.m_TransitionConstantArray
            if not transitions:
                break
            transition = transitions[0].data
            if len(transitions) != 1 or transition.m_ConditionConstantArray or not transition.m_HasExitTime or transition.m_ExitTime != 1 or transition.m_TransitionDuration != 0:
                raise ValueError("气泡 Animator 使用了未知切换规则")
            state_index = transition.m_DestinationState
        frames = []
        duration = 0.0
        loop_start = None
        timed_frames = []
        for clip in clips:
            if clip.m_MuscleClip.m_LoopTime:
                loop_start = duration
            timed_frames.extend((duration + time, reference) for time, reference in animation_frames(clip))
            duration += clip.m_MuscleClip.m_StopTime
        for time, reference in timed_frames:
            sprite = reference.read()
            relative = f"assets/moods/{pid}/{reference.path_id}.png"
            path = output / relative
            if not path.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
                # Restore the Sprite's trimmed transparent margins, so frames
                # retain their native pivot and do not wobble while playing.
                image = Image.new("RGBA", (round(sprite.m_Rect.width), round(sprite.m_Rect.height)))
                cropped = sprite.image
                trim = sprite.m_RD.textureRectOffset
                image.paste(cropped, (round(trim.x), image.height - round(trim.y) - cropped.height))
                image.save(path)
            frames.append({"time": time, "file": relative})
        result = {
            "frames": frames,
            "duration": duration,
            "loop_start": loop_start,
            "size": [rect.m_SizeDelta.x, rect.m_SizeDelta.y],
            "offset": [rect.m_AnchoredPosition.x, rect.m_AnchoredPosition.y],
            "pivot": [rect.m_Pivot.x, rect.m_Pivot.y],
            "scale": [rect.m_LocalScale.x, rect.m_LocalScale.y],
        }
        animations[pid] = result
        return result

    moods = {}
    for _ in range(count):
        right_file, right, left_file, left, mappings = struct.unpack_from("<iqiqi", raw, offset)
        offset += 28
        if right_file or left_file:
            raise ValueError("气泡预制体使用了未知外部引用")
        entry = {"left": animation(left), "right": animation(right)}
        for _ in range(mappings):
            file_id, pid = struct.unpack_from("<iq", raw, offset)
            offset += 12
            if file_id:
                raise ValueError("气泡表情映射使用了未知外部引用")
            mapping = objects[pid].get_raw_data()
            _, pos = _read_str(mapping, 28)
            _, pos = _read_str(mapping, pos)
            key, _ = _read_str(mapping, pos)
            moods[key] = entry
    return moods
