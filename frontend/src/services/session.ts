import { ref } from 'vue'

// Mirrors localStorage so views (like the top bar) react to role changes.
const admin = ref(localStorage.getItem('is_admin') === '1')

export function getToken(): string | null {
  return localStorage.getItem('token')
}

export function getRefreshToken(): string | null {
  return localStorage.getItem('refresh_token')
}

export function saveTokens(token: string, refreshToken: string): void {
  localStorage.setItem('token', token)
  localStorage.setItem('refresh_token', refreshToken)
}

export function saveSession(token: string, refreshToken: string, isAdmin: boolean): void {
  saveTokens(token, refreshToken)
  setAdmin(isAdmin)
}

export function setAdmin(isAdmin: boolean): void {
  localStorage.setItem('is_admin', isAdmin ? '1' : '0')
  admin.value = isAdmin
}

export function clearSession(): void {
  localStorage.removeItem('token')
  localStorage.removeItem('refresh_token')
  localStorage.removeItem('is_admin')
  admin.value = false
}

export function isAdmin(): boolean {
  return admin.value
}
