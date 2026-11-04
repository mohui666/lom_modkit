using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using HarmonyLib;
using Mortal.Combat;
using UnityEngine;
using UObject = UnityEngine.Object;

namespace MortalModHost
{
    /// <summary>PNG frames replace only the acting character; native HandleDamage runs at the configured frame.</summary>
    internal static class CustomCombatAnimationRuntime
    {
        private static readonly Dictionary<UserContent, Sprite[]> Clips = new Dictionary<UserContent, Sprite[]>();
        private static readonly List<UObject> Owned = new List<UObject>();
        private static readonly List<CustomCombatFramePlayer> Playing = new List<CustomCombatFramePlayer>();

        internal static void Validate(UserContent content)
        {
            var animation = content.Gameplay.Skill.Animation;
            if (animation == null) return;
            foreach (string file in animation.Frames)
            {
                byte[] bytes;
                if (content.Files == null || !content.Files.TryGetValue(file, out bytes) || bytes == null || bytes.Length < 24
                    || bytes.Length > 8 * 1024 * 1024 || bytes[0] != 137 || bytes[1] != 80 || bytes[2] != 78 || bytes[3] != 71)
                    throw new InvalidOperationException("招式动画缺少有效PNG帧：" + content.Id + "/" + file);
            }
        }

        internal static void Preload(UserContent content)
        {
            if (content.Gameplay.Skill.Animation == null || Clips.ContainsKey(content)) return;
            Validate(content);
            var frames = new List<Sprite>();
            var distinct = new Dictionary<string, Sprite>(StringComparer.OrdinalIgnoreCase);
            foreach (string file in content.Gameplay.Skill.Animation.Frames)
            {
                Sprite sprite;
                if (!distinct.TryGetValue(file, out sprite))
                {
                    var texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
                    Owned.Add(texture);
                    if (!texture.LoadImage(content.Files[file]))
                        throw new InvalidOperationException("招式动画PNG解码失败：" + content.Id + "/" + file);
                    texture.name = content.Id + "/" + file;
                    sprite = Sprite.Create(texture, new Rect(0, 0, texture.width, texture.height), new Vector2(.5f, 0), 100);
                    sprite.name = texture.name;
                    Owned.Add(sprite);
                    distinct.Add(file, sprite);
                }
                frames.Add(sprite);
            }
            Clips.Add(content, frames.ToArray());
        }

        internal static void Clear()
        {
            foreach (var player in Playing.ToArray())
                if (player != null) { player.Restore(); UObject.Destroy(player.gameObject); }
            Playing.Clear();
            Clips.Clear();
            for (int i = Owned.Count - 1; i >= 0; i--)
                if (Owned[i] != null) UObject.Destroy(Owned[i]);
            Owned.Clear();
        }

        private static IEnumerator Play(CombatCharacterController character, CustomCombatSkillRuntime.Skill skill, int[] damage, bool hit)
        {
            Sprite[] frames = Clips[skill.Content];
            var definition = skill.Data.Animation;
            var root = new GameObject("ModSkillAnimation_" + skill.Content.Id);
            var player = root.AddComponent<CustomCombatFramePlayer>();
            Playing.Add(player);
            CustomGameplayRuntime.Set(character, "_damageArray", damage);
            CustomGameplayRuntime.Set(character, "_isHit", hit);
            character.SetAnimationStart();
            Coroutine damageRoutine = null;
            bool struck = false;
            try
            {
                player.Setup(character, frames, definition.Scale);
                float start = Time.time;
                float duration = frames.Length / (float)definition.Fps;
                do
                {
                    int frame = Math.Min(frames.Length - 1, (int)((Time.time - start) * definition.Fps));
                    player.SetFrame(frame);
                    if (!struck && frame >= definition.HitFrame)
                    {
                        struck = true;
                        character.TakeDamage(0);
                        damageRoutine = character.StartCoroutine(character.HandleDamage());
                        Debug.Log("[CustomSkill] animation-hit=" + skill.Content.Id + " frame=" + frame);
                    }
                    yield return null;
                } while (Time.time - start < duration);
                if (damageRoutine != null) yield return damageRoutine;
            }
            finally
            {
                player.Restore();
                Playing.Remove(player);
                UObject.Destroy(root);
                character.SetAnimationEnd();
                character.SetIdle(true);
            }
        }

        [HarmonyPatch]
        private static class CharacterActionPatch
        {
            private static IEnumerable<MethodBase> TargetMethods()
            {
                // These are the concrete native overrides, not just the unused base iterator.
                foreach (Type type in new[] { typeof(CombatPlayerController), typeof(CombatPlayerRootController), typeof(CombatEnemyController) })
                    foreach (string method in new[] { "Attack", "Weapon", "Ultimate" })
                        yield return AccessTools.DeclaredMethod(type, method);
            }
            private static bool Prefix(CombatCharacterController __instance, object[] __args, ref IEnumerator __result)
            {
                var actor = (CombatActionController)AccessTools.Field(typeof(CombatCharacterController), "_sourceAction").GetValue(__instance);
                CustomCombatSkillRuntime.Skill skill;
                if (!CustomCombatSkillRuntime.TryActive(actor, out skill) || skill.Data.Animation == null) return true;
                int[] damage = __args[0] as int[] ?? new[] { (int)__args[0] };
                __result = Play(__instance, skill, damage, (bool)__args[1]);
                return false;
            }
        }
    }

    [DefaultExecutionOrder(30000)]
    internal sealed class CustomCombatFramePlayer : MonoBehaviour
    {
        private SpriteRenderer[] _hidden;
        private bool[] _enabled;
        private Renderer[] _modelHidden;
        private bool[] _modelEnabled;
        private SpriteRenderer _renderer;
        private Sprite[] _frames;
        private CombatCharacterController _character;
        private Transform _anchor;
        private Vector3 _offset;
        private float _height, _scale;
        private int _frame;
        private bool _restored;

        internal void Setup(CombatCharacterController character, Sprite[] frames, int scale)
        {
            _character = character;
            _frames = frames;
            _scale = scale / 100f;
            _hidden = character.GetComponentsInChildren<SpriteRenderer>(true);
            _enabled = _hidden.Select(r => r.enabled).ToArray();
            _modelHidden = character.GetComponentsInChildren<Renderer>(true).Where(r => !(r is SpriteRenderer)).ToArray();
            _modelEnabled = _modelHidden.Select(r => r.enabled).ToArray();
            // Native player animation branches can hide idle before this iterator begins.
            // Renderer.bounds can then be empty; the sprite's own bounds and current transform
            // still describe the real character without activating or editing its renderers.
            // MaxModel meshes use their own rendering space/layer. The replacement is a
            // 2D sprite, so keep the native sprite branch's proven camera space even when
            // that branch is inactive; model renderers are hidden separately above.
            SpriteRenderer source = _hidden.Where(r => r.sprite != null)
                .OrderByDescending(r => r.enabled && r.gameObject.activeInHierarchy)
                .ThenByDescending(r => SpriteWorldBounds(r).size.y).FirstOrDefault();
            if (source == null) throw new InvalidOperationException("当前决斗角色没有可替换的SpriteRenderer");
            Bounds bounds = SpriteWorldBounds(source);
            if (bounds.size.y <= 0) throw new InvalidOperationException("当前决斗角色的渲染器没有有效高度");
            _anchor = character.transform;
            _offset = new Vector3(bounds.center.x, bounds.min.y, bounds.center.z) - _anchor.position;
            _height = bounds.size.y;
            gameObject.layer = source.gameObject.layer;
            _renderer = gameObject.AddComponent<SpriteRenderer>();
            _renderer.sortingLayerID = source.sortingLayerID;
            _renderer.sortingOrder = source.sortingOrder + 1;
            // Skill PNG frames face left. PlayerRoot's inactive sprite branch can have
            // different mirroring from its visible MaxModel, so use the actual duel side.
            var actor = (CombatActionController)AccessTools.Field(typeof(CombatCharacterController), "_sourceAction").GetValue(character);
            _renderer.flipX = actor == CombatManager.Instance.GetActionController(CombatCharacterType.我方);
            _renderer.flipY = source.flipY ^ (source.transform.TransformVector(Vector3.up).y < 0);
            SetFrame(0);
            LateUpdate();
        }

        private static Bounds SpriteWorldBounds(SpriteRenderer source)
        {
            Bounds local = source.sprite.bounds;
            Vector3 min = local.min, max = local.max;
            var world = new Bounds();
            bool first = true;
            for (int x = 0; x < 2; x++)
                for (int y = 0; y < 2; y++)
                    for (int z = 0; z < 2; z++)
                    {
                        var point = new Vector3(x == 0 ? min.x : max.x, y == 0 ? min.y : max.y, z == 0 ? min.z : max.z);
                        if (source.flipX) point.x = -point.x;
                        if (source.flipY) point.y = -point.y;
                        point = source.transform.TransformPoint(point);
                        if (first) { world = new Bounds(point, Vector3.zero); first = false; }
                        else world.Encapsulate(point);
                    }
            return world;
        }

        internal void SetFrame(int frame) { _frame = frame; }
        private void LateUpdate()
        {
            if (_restored || _renderer == null || _anchor == null) return;
            foreach (SpriteRenderer part in _hidden) if (part != null) part.enabled = false;
            foreach (Renderer part in _modelHidden) if (part != null) part.enabled = false;
            _renderer.sprite = _frames[_frame];
            transform.position = _anchor.position + _offset;
            float size = _height * _scale / _frames[0].bounds.size.y;
            transform.localScale = Vector3.one * size;
        }

        internal void Restore()
        {
            if (_restored) return;
            _restored = true;
            if (_renderer != null) _renderer.enabled = false;
            if (_hidden != null)
                for (int i = 0; i < _hidden.Length; i++) if (_hidden[i] != null) _hidden[i].enabled = _enabled[i];
            if (_modelHidden != null)
                for (int i = 0; i < _modelHidden.Length; i++) if (_modelHidden[i] != null) _modelHidden[i].enabled = _modelEnabled[i];
            if (_character != null) _character.SetAnimationEnd();
        }
        private void OnDestroy() { Restore(); }
    }
}
