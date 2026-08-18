namespace MortalModHost
{
    /// <summary>
    /// CombatStat stores maximum and starting vitality separately. A node that only
    /// provides a maximum starts at that full value instead of inheriting the
    /// temporary Combat shell's old starting value.
    /// </summary>
    internal static class CombatVitalPolicy
    {
        /// <summary>
        /// player_max_health is an extra base amount. The official CombatManager
        /// calculation has already applied vitality and passive bonuses before
        /// this value is added.
        /// </summary>
        internal static int AddHealthBaseBonus(int officialMaximum, int extraBase)
        {
            long total = (long)officialMaximum + extraBase;
            if (total <= 0) return 0;
            return total >= int.MaxValue ? int.MaxValue : (int)total;
        }

        internal static void Resolve(
            int inheritedMaximum,
            int inheritedCurrent,
            bool hasConfiguredMaximum,
            int configuredMaximum,
            bool hasConfiguredCurrent,
            int configuredCurrent,
            out int maximum,
            out int current)
        {
            maximum = hasConfiguredMaximum ? configuredMaximum : inheritedMaximum;
            current = hasConfiguredCurrent
                ? configuredCurrent
                : (hasConfiguredMaximum ? maximum : inheritedCurrent);

            if (maximum <= 0)
            {
                current = 0;
                return;
            }
            if (current < 0) current = 0;
            else if (current > maximum) current = maximum;
        }

        /// <summary>
        /// CombatStatItem keeps the configured base and the original effect
        /// modifiers separately. Once InitSkill has populated ModifyList, an
        /// omitted starting health means full final health. An explicit starting
        /// health, or an Init effect that already changed health, is preserved.
        /// </summary>
        internal static int ResolveInitialHealthAfterModifiers(
            int current,
            bool preserveCurrent,
            int finalMaximum)
        {
            if (finalMaximum <= 0) return 0;
            if (!preserveCurrent) return finalMaximum;
            if (current < 0) return 0;
            return current > finalMaximum ? finalMaximum : current;
        }
    }
}
