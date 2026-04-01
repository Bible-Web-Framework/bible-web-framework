import { LTR_REGEX, RTL_REGEX } from './data/textDir'

export function excelColumnName(column: number) {
  let columnName = ''
  while (column) {
    const modulo = (column - 1) % 26
    columnName = String.fromCharCode('a'.charCodeAt(0) + modulo) + columnName
    column = Math.floor((column - modulo) / 26)
  }
  return columnName
}

// Based on https://github.com/chromium/chromium/blob/2cb88d767d7df2225ce1b531019d65c07d6261c0/base/i18n/rtl.cc#L250
export function getAutoTextDir(text: string) {
  for (const char of text) {
    if (LTR_REGEX.test(char)) {
      return 'ltr'
    }
    if (RTL_REGEX.test(char)) {
      return 'rtl'
    }
  }
  return undefined
}
