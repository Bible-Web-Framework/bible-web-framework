export function excelColumnName(column: number) {
  let columnName = ''
  while (column) {
    const modulo = (column - 1) % 26
    columnName = String.fromCharCode('a'.charCodeAt(0) + modulo) + columnName
    column = Math.floor((column - modulo) / 26)
  }
  return columnName
}
