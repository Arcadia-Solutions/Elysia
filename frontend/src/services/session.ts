export function getToken(): string | null {
  return localStorage.getItem('token')
}

export function saveSession(token: string, isAdmin: boolean): void {
  localStorage.setItem('token', token)
  localStorage.setItem('is_admin', isAdmin ? '1' : '0')
}

export function clearSession(): void {
  localStorage.removeItem('token')
  localStorage.removeItem('is_admin')
}

export function isAdmin(): boolean {
  return localStorage.getItem('is_admin') === '1'
}
