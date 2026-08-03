//! Rule kind discriminant shared with ruleset loading.


#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    CyclomaticComplexity,
    NPathComplexity,
    ExcessiveMethodLength,
    ExcessiveClassLength,
    ExcessiveParameterList,
    ExcessivePublicCount,
    TooManyFields,
    TooManyMethods,
    TooManyPublicMethods,
    ExcessiveClassComplexity,
    ShortClassName,
    LongClassName,
    ShortVariable,
    LongVariable,
    ShortMethodName,
    ConstantNamingConventions,
    BooleanGetMethodName,
    UnusedPrivateField,
    UnusedLocalVariable,
    UnusedPrivateMethod,
    UnusedFormalParameter,
    BooleanArgumentFlag,
    ElseExpression,
    IfStatementAssignment,
    DuplicatedArrayKey,
    StaticAccess,
    ExitExpression,
    GotoStatement,
    CountInLoopExpression,
    DevelopmentCodeFragment,
    EmptyCatchBlock,
    CouplingBetweenObjects,
    GlobalVariable,
    LackOfCohesionOfMethods,
    CamelCaseClassName,
    CamelCaseMethodName,
    CamelCasePropertyName,
    CamelCaseParameterName,
    CamelCaseVariableName,
}


impl RuleKind {
    pub(crate) const COUNT: usize = Self::CamelCaseVariableName as usize + 1;
}

