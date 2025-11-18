// ===== Lexer/Parser feature showcase =====
// 行注释 // 与块注释 /* ... */ 都应被识别
/* 多行
   注释 */

#define MAXN 8   // 预处理行会被作为 Preprocessor token（当前不会真正处理）

typedef unsigned long size_t;
typedef int MyInt, *MyIntPtr;
typedef MyInt Matrix[2][3];

size_t g_len;
MyInt   g_val = 42;
Matrix  mat = { {1,2,3}, {4,5,6} };

int main() {
  MyInt x = 1;
  MyIntPtr p = &x;
  size_t n = sizeof(size_t);
  Matrix m = { {1,2,3}, {4,5,6} };

  x = (MyInt) (x + 3);
  return (int)(x + *p + (int)n + m[1][2]);
}


// ==== 全局声明（多声明、指针、多维数组、可选维度） ====
int g, h = 1, *pg;
int grid[3][2], table[][4];      // 第二个维度指定，第一维留空（仅语法测试）

char msg[16] = "hello";          // 字符串常量
double *gp = 0;

// ==== 函数：参数含指针与数组形参、多维数组 ====
int sum2d(int *row[4], int n);
int process(int a[][4], int n, char *s);

int sum2d(int *row[4], int n) {
  int acc = 0;
  for (int i = 0; i < n; i = i + 1) {
    // 下标 + 解引用 + 逗号表达式 + 后缀自增
    acc = acc + (row[i][0] + row[i][1]), i++;
  }
  return acc;
}

// 一个“结构体”风格的成员访问语法测试（本解析器不做类型检查，这里仅测语法）
int member_ops() {
  int x = 3;
  double *p = (double*)0;   // C 风格强转 + 指针
  x = (int)(x + 2.5);       // 括号内 cast + 算术

  // 一元运算（前缀/后缀）
  ++x;
  x--;
  x = +x + -x + ~x;
  x = !x;

  // sizeof / alignof：对表达式、对类型
  int s1 = sizeof x;
  int s2 = sizeof(int**);
  int a1 = alignof x;       // 作为扩展形态支持 alignof expr
  int a2 = alignof(double*);

  // 成员与指针成员（仅语法）
  obj.member = x;
  pobj->field = s1;

  // 逗号表达式与条件运算符
  int y = (x = x + 1, x * 2);
  int z = (y > 10) ? y : 10;

  // 函数调用测试
  foo(x, y, z);

  return x + y + z + s1 + s2 + a1 + a2;
}

int control_flow(int n) {
  int i = 0, acc = 0;

  while (i < n) {
    acc = acc + i;
    i++;
  }

  for (int j = 0; j < n; j = j + 1) {
    if (j % 2 == 0) {
      continue;
    } else {
      acc = acc + j;
    }
  }

  switch (n) {
    case 0:
      acc = acc + 100;
      break;
    case 1:
    case 2:
      acc = acc + 200;
      break;
    default:
      acc = acc + 300;
      break;
  }

  return acc;
}

// 多维数组参数 + 字符串参数 + 指针参数
int process(int a[][4], int n, char *s) {
  int t = 0;
  for (int i = 0; i < n; i = i + 1) {
    t = t + a[i][0] + a[i][1] + a[i][2] + a[i][3];
  }
  // 简单成员/指针成员/数组下标表达式都用一下（语法测试）
  node.next->value = t;
  buf[0][1] = t;
  return t;
}

// 主函数：把上面的片段都走一遍
int main() {
  int local = 3;
  int *rows[4];
  int mat[2][4];
  char *str = "world";

  mat[0][0] = 1; mat[0][1] = 2; mat[0][2] = 3; mat[0][3] = 4;
  mat[1][0] = 5; mat[1][1] = 6; mat[1][2] = 7; mat[1][3] = 8;

  rows[0] = &mat[0][0];
  rows[1] = &mat[0][2];
  rows[2] = &mat[1][0];
  rows[3] = &mat[1][2];

  g = sum2d(rows, 4);
  h = process(mat, 2, str);

  local = member_ops();
  local = local + control_flow(5);

  // 逗号 + 赋值复合运算
  local += (g = g + h, h = h + 1, g);

  return local;
}

/* ================== 错误用例区（用于测试 caret 报错） ==================
   想测试错误，请把下面 #if 0 改为 #if 1
*/
#if 0
int bad_cases() {
  int x = 0
  x = (int x) + 1;          // 错误：把 (type) 写成 (int x)
  int y = sizeof(int*       // 错误：少 ')'
  z = alignof( int**        // 错误：少 ')'
  if (x > 0 {               // 错误：少 ')'
    x++;
  }
  arr[3[2]] = 1;            // 错误：中括号不匹配
  return x;
}
#endif
