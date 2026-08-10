import { useState, useCallback, useMemo } from "react";
import type { AppId } from "@/lib/api";
import { useProvidersQuery } from "@/lib/query/queries";

interface UseKimicodeFormStateParams {
  initialData?: {
    settingsConfig?: Record<string, unknown>;
  };
  appId: AppId;
  providerId?: string;
  onSettingsConfigChange: (config: string) => void;
  getSettingsConfig: () => string;
}

export interface KimicodeFormState {
  kimicodeProviderKey: string;
  setKimicodeProviderKey: (key: string) => void;
  existingKimicodeKeys: string[];
  resetKimicodeState: () => void;
}

export function useKimicodeFormState(
  params: UseKimicodeFormStateParams,
): KimicodeFormState {
  const { appId, providerId } = params;
  const { data: kimicodeProvidersData } = useProvidersQuery("kimicode");

  const existingKimicodeKeys = useMemo(() => {
    if (!kimicodeProvidersData?.providers) return [];
    return Object.keys(kimicodeProvidersData.providers).filter(
      (k) => k !== providerId,
    );
  }, [kimicodeProvidersData?.providers, providerId]);

  const [kimicodeProviderKey, setKimicodeProviderKey] = useState<string>(() => {
    if (appId !== "kimicode") return "";
    return providerId || "";
  });

  const resetKimicodeState = useCallback(() => {
    setKimicodeProviderKey("");
  }, []);

  return {
    kimicodeProviderKey,
    setKimicodeProviderKey,
    existingKimicodeKeys,
    resetKimicodeState,
  };
}
