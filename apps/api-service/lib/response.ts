import { NextResponse } from 'next/server';

/**
 * Standard API response format
 */
export interface ApiResponse<T = unknown> {
  success: boolean;
  data: T | undefined;
  code: number;
  error: string;
}

/**
 * Business error codes
 */
export const ErrorCode = {
  // Success
  SUCCESS: 0,

  // Client errors (10000-19999)
  INVALID_REQUEST: 10000,
  UNAUTHORIZED: 10001,
  NOT_FOUND: 10002,
  ACTION_NOT_FOUND: 10003,

  // Server errors (20000-29999)
  INTERNAL_ERROR: 20000,
  DATABASE_ERROR: 20001,
} as const;

export type ErrorCodeValue = (typeof ErrorCode)[keyof typeof ErrorCode];

/**
 * Create a successful response
 */
export function successResponse<T>(
  data: T,
  statusCode: number = 200
): NextResponse<ApiResponse<T>> {
  return NextResponse.json(
    {
      success: true,
      data,
      code: ErrorCode.SUCCESS,
      error: '',
    },
    { status: statusCode }
  );
}

/**
 * Create an error response
 */
export function errorResponse(
  code: ErrorCodeValue,
  error: string,
  statusCode: number = 500
): NextResponse<ApiResponse<undefined>> {
  return NextResponse.json(
    {
      success: false,
      data: undefined,
      code,
      error,
    },
    { status: statusCode }
  );
}

/**
 * Convenience functions for common error cases
 */
export function invalidRequestResponse(
  message: string = 'Invalid request'
): NextResponse<ApiResponse<undefined>> {
  return errorResponse(ErrorCode.INVALID_REQUEST, message, 400);
}

export function unauthorizedResponse(
  message: string = 'Unauthorized'
): NextResponse<ApiResponse<undefined>> {
  return errorResponse(ErrorCode.UNAUTHORIZED, message, 401);
}

export function notFoundResponse(
  message: string = 'Resource not found'
): NextResponse<ApiResponse<undefined>> {
  return errorResponse(ErrorCode.NOT_FOUND, message, 404);
}

export function actionNotFoundResponse(
  message: string = 'Action not found'
): NextResponse<ApiResponse<undefined>> {
  return errorResponse(ErrorCode.ACTION_NOT_FOUND, message, 404);
}

export function internalErrorResponse(
  message: string = 'Internal server error'
): NextResponse<ApiResponse<undefined>> {
  return errorResponse(ErrorCode.INTERNAL_ERROR, message, 500);
}

export function databaseErrorResponse(
  message: string = 'Database error'
): NextResponse<ApiResponse<undefined>> {
  return errorResponse(ErrorCode.DATABASE_ERROR, message, 500);
}
