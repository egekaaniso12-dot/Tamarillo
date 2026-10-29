import { invoke } from '@tauri-apps/api/core';
export type AppInfo = { name: string; copyright: string };
export const getAppInfo = () => invoke<AppInfo>('app_info');
