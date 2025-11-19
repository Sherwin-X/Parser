typedef unsigned long size_t;
typedef int MyInt, *MyIntPtr;
typedef MyInt Matrix2x3[2][3];

typedef struct Point Point;
typedef enum Color Color;
typedef union Value Value;

struct Point *gp;
enum Color gc;
union Value gv;

int g_arr1[3] = { 1, 2, 3 };
int g_arr2[2][3] = {
    { 1, 2, 3 },
    { 4, 5, 6 },
};

Matrix2x3 g_mat = {
    { 10, 20, 30 },
    { 40, 50, 60 },
};

int add(MyInt a, MyInt b) {
    return a + b;
}

int main() {
    MyInt x = 1;
    MyIntPtr p = &x;

    size_t n1 = sizeof(size_t);
    size_t n2 = sizeof(struct Point*);
    size_t a1 = alignof(int);
    size_t a2 = alignof(struct Point*);

    enum Color *pc = &gc;

    int i;
    for (i = 0; i < 3; i = i + 1) {
        g_arr1[i] = g_arr1[i] * 2;
    }

    int sum = 0;
    int r, c;
    for (r = 0; r < 2; r = r + 1) {
        for (c = 0; c < 3; c = c + 1) {
            sum = sum + g_mat[r][c];
        }
    }

    int cond = 0;
    int t = cond ? 1 : 2;

    switch (t) {
        case 1:
            sum = sum + 100;
            break;
        case 2:
            sum = sum + 200;
            /* fallthrough */
        default:
            sum = sum + 300;
            break;
    }

    // 测试 cast / 指针 / struct/union/enum 标签类型
    struct Point *p0 = (struct Point*)0;
    Value *v0 = (Value*)0;
    Color *c0 = (Color*)pc;

    return add(sum, (MyInt)(*p + (MyInt)n2 + (MyInt)a1 + (MyInt)a2));
}
