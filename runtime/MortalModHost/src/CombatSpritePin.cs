using UnityEngine;

namespace MortalModHost
{
    /// <summary>
    /// 原版 CombatEnemyController.ChangeAnimationState 只是 Animator.Play。
    /// Attack/Hurt 剪辑会改另一套 SpriteRenderer 的 localPosition。无独立四帧时
    /// 必须在 LateUpdate 把所有渲染器钉回待机局部变换，动画事件仍可结束。
    /// </summary>
    internal sealed class CombatSpritePin : MonoBehaviour
    {
        internal struct Pin
        {
            internal Transform Transform;
            internal Vector3 LocalPosition;
            internal Quaternion LocalRotation;
            internal Vector3 LocalScale;
        }

        internal Pin[] Pins;

        private void LateUpdate()
        {
            Apply();
        }

        internal void Apply()
        {
            if (Pins == null) return;
            for (int i = 0; i < Pins.Length; i++)
            {
                Transform target = Pins[i].Transform;
                if (target == null) continue;
                target.localPosition = Pins[i].LocalPosition;
                target.localRotation = Pins[i].LocalRotation;
                target.localScale = Pins[i].LocalScale;
            }
        }
    }
}
