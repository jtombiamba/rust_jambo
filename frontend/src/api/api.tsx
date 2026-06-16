import axios from "axios";
import { getApiBaseUrl } from "../utils/runtimeConfig";

let accessToken: string | null = null;

export function setAccessToken(token: string | null) {
  accessToken = token;
}

export function getAccessToken() {
  return accessToken;
}

axios.defaults.withCredentials = true;
axios.defaults.baseURL = getApiBaseUrl();

axios.interceptors.request.use((config) => {
  if (accessToken) {
    config.headers.Authorization = `Bearer ${accessToken}`;
  }
  return config;
});

export { axios as api };
