import { useQuery } from "@tanstack/react-query";
import { providersApi } from "@/lib/api/providers";

/**
 * Centralized query keys for all Kimi Code-related queries.
 */
export const kimicodeKeys = {
  all: ["kimicode"] as const,
  liveProviderIds: ["kimicodeLiveProviderIds"] as const,
};

/**
 * Query live provider IDs from Kimi Code live config.
 * Used by ProviderList to show "In Config" badge.
 */
export function useKimicodeLiveProviderIds(enabled: boolean) {
  return useQuery({
    queryKey: kimicodeKeys.liveProviderIds,
    queryFn: () => providersApi.getKimicodeLiveProviderIds(),
    enabled,
  });
}
