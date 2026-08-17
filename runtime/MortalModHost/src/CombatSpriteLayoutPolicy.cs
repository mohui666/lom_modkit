using System;

namespace MortalModHost
{
    /// <summary>
    /// 自定义决斗图与官方 Combat 占位框的对齐规则。官方 AABB 底部含空白，
    /// 贴底会把自定义角色压进指令栏；脚底对齐到占位框内一条抬高的站位线。
    /// </summary>
    internal static class CombatSpriteLayoutPolicy
    {
        internal static float FitScale(
            float targetWidth, float targetHeight, float currentWidth, float currentHeight)
        {
            if (targetWidth <= 0.001f || targetHeight <= 0.001f
                || currentWidth <= 0.001f || currentHeight <= 0.001f)
                return 0f;
            float scale = Math.Min(targetWidth / currentWidth, targetHeight / currentHeight);
            return float.IsNaN(scale) || float.IsInfinity(scale) || scale <= 0f ? 0f : scale;
        }

        internal static float AlignCenterX(float targetCenterX, float currentCenterX)
        {
            return targetCenterX - currentCenterX;
        }

        /// <summary>
        /// 无独立四帧时，立绘必须叠在官方待机 Sprite 的中心。贴底/站位线都会
        /// 把半身肖像压进指令栏；官方 Attack/Hurt 动画再移动另一套 Renderer，
        /// 同一张图就会整张上移。
        /// </summary>
        internal static float AlignCenterY(float targetCenterY, float currentCenterY)
        {
            return targetCenterY - currentCenterY;
        }

        internal static int SharedIdleIndex(bool[] valid)
        {
            if (valid == null || valid.Length == 0) return -1;
            if (valid[0]) return 0;
            for (int i = 1; i < valid.Length; i++)
                if (valid[i]) return i;
            return -1;
        }
    }
}
